//
//           _______________________________ ________
//           \____    /\_   _____/\______   \\_____  \
//             /     /  |    __)_  |       _/ /   |   \
//            /     /_  |        \ |    |   \/    |    \
//           /_______ \/_______  / |____|_  /\_______  /
//                   \/        \/         \/         \/
//           Z  E  R  O  .  I  O     N  E  T  W  O  R  K
//           © C O P Y R I O T   2 0 7 5 @ Z E R O . I O

// 1. create campaigns with custom funding goal and runtime
// 2. invest into open campaigns

#![cfg_attr(not(feature = "std"), no_std)]

// TODO: harden checks on completion
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
// only on nightly
// #![feature(const_fn_fn_ptr_basics)]

use frame_support::{
	decl_error, decl_event, decl_module, decl_storage,
	ensure,
	dispatch::DispatchResult,
	traits::{
		// EnsureOrigin,
		Randomness,
		Currency,
		ExistenceRequirement,
		ReservableCurrency,
		Get
	},
	storage::child::exists
};
use frame_system::{ self as system, ensure_signed };
use sp_core::{ Hasher, H256 };
use sp_runtime::{
	traits::{ Hash, TrailingZeroInput },
	ModuleId,
};
use sp_std::prelude::*;
use codec::{ Encode, Decode };


// TODO: tests
// #[cfg(test)]
// mod tests;

// TODO: pallet benchmarking
// mod benchmarking;

// TODO: weights
// mod default_weights;

// TODO: externalise error messages

// mod constants;

// module header
const PALLET_ID: ModuleId = ModuleId(*b"zerocrwd");
const PALLET_VERSION: &str = "1.0";
// limit simultanous campaign initiation
const MAX_CONTRIBUTIONS_PER_BLOCK: usize = 5;
// max campaign duration
const MAX_CAMPAIGN_LENGTH: u64 = 777600;

pub trait Trait: system::Trait + balances::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
	type Currency: ReservableCurrency<Self::AccountId>;
	type Randomness: Randomness<Self::Hash>;
	type MinLength: Get<usize>;
	type MaxLength: Get<usize>;
	type Nonce: Get<u64>;
	// TODO: collect fees and send to treasury
	// type CreationFee: Get<T::Balance<Self>>;
	// type ContributionFee: Get<T::Balance<Self>>;
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq)]
pub struct Campaign<Hash, AccountId, Balance, BlockNumber> {
	/// unique hash to identify campaign
	id: Hash,
	/// controller account of the campaign
	manager: AccountId,
	/// campaign owners deposit
	deposit: Balance,
	/// blocknumber until campaign has to reach cap
	expiry: BlockNumber,
	/// minimum amount of token to become a successful campaign
	cap: Balance,
	/// name
	// TODO: replace with ipfs hash
	name: Vec<u8>,
	/// protocol: 0 grant, 1 prepaid, 2 loan, 3 security, 4 dao
	protocol: u8,
	/// status: 0 not started, 1 in progress, 2 success, 3 fail
	status: u8,
	// /// token symbol
	// token_symbol: Vec<u8>,
	// /// token name
	// token_name: Vec<u8>,
}

decl_storage! {
	trait Store for Module<T: Trait> as CrowdfundingFactory {

		// TODO:
		//
		// -	statistics:
		// 		total campaigns
		// 		total (indiviual?) contributors sum
		// 		total contributions sum
		//
		// -	all / campaigns meta info
		//
		// -	allCampaignsArray { contributor hash, contribution amount } =>
		// -	allCampaignsCount
		//
		// - 	ownedCampaigns

		/// Get one or all Campaigns
		Campaigns get(fn campaign_by_id): map hasher(blake2_128_concat) T::Hash => Campaign<T::Hash, T::AccountId, T::Balance, T::BlockNumber>;

		/// Get Campaign owner by campaign id
		CampaignOwner get(fn owner_of): map hasher(blake2_128_concat) T::Hash => Option<T::AccountId>;

		/// Max campaign block limit
		CampaignMaxDuration get(fn get_max_duration) config(): T::BlockNumber = T::BlockNumber::from(MAX_CAMPAIGN_LENGTH);

		/// Campaigns ending in block x
		CampaignsByBlockNumber get(fn campaign_expire_at): map hasher(blake2_128_concat) T::BlockNumber => Vec<T::Hash>;

		/// Campaign state
	 	/// status: 0 not started, 1 in progress, 2 success, 3 fail
		CampaignState get(fn state_of): map hasher(blake2_128_concat) T::Hash => u8;

		// total number of campaigns
		AllCampaignsCount get(fn all_campaigns_count): u64;
		AllCampaignsArray get(fn campaigns_by_index): map hasher(blake2_128_concat) u64 => T::Hash;
		AllCampaignsIndex: map hasher(blake2_128_concat) T::Hash => u64;

		// caller owned campaigns
		OwnedCampaignsArray get(fn owned_campaigns_by_index): map hasher(blake2_128_concat) (T::AccountId, u64) => T::Hash;
		OwnedCampaignsCount get(fn owned_campaigns_count): map hasher(blake2_128_concat) T::AccountId => u64;
		OwnedCampaignsIndex: map hasher(blake2_128_concat) (T::AccountId, T::Hash) => u64;

		// caller contributed campaigns
		ContributedCampaignsArray get(fn contributed_campaigns_by_index): map hasher(blake2_128_concat) (T::AccountId, u64) => T::Hash;
		ContributedCampaignsCount get(fn contributed_campaigns_count): map hasher(blake2_128_concat) T::AccountId => u64;
		ContributedCampaignsIndex: map hasher(blake2_128_concat) (T::AccountId, T::Hash) => u64;

		// Total contributions per campaign
		TotalContributions get(fn total_contributions): map hasher(blake2_128_concat) T::Hash => T::Balance;

		// Contributions per user
		ContributedAmount get(fn contributed_amount): map hasher(blake2_128_concat) (T::Hash, T::AccountId) => T::Balance;

		// Contributor Accounts
		ContributorAccounts get(fn contributor_accounts): map hasher(blake2_128_concat) T::Hash => Vec<T::AccountId>;
		ContributorAccountsCount get(fn contributor_accounts_count): map hasher(blake2_128_concat) T::Hash => u64;

		// Campaign nonce, increases per created campaign
		Nonce: u64;
	}
}

decl_event! {
	pub enum Event<T> where
		<T as system::Trait>::Hash,
		<T as system::Trait>::AccountId,
		<T as balances::Trait>::Balance,
		<T as system::Trait>::BlockNumber,
		EventMessage = Vec<u8>,
	{
		CampaignCreated(Hash, AccountId, Balance, Balance, BlockNumber, Vec<u8>),
		CampaignContributed(Hash, AccountId, Balance, BlockNumber),
		CampaignFinalized(Hash, Balance, BlockNumber, bool),
		CampaignFailed(Hash, Balance, BlockNumber, bool),
		CampaignUpdated(Hash, u8, BlockNumber),
		Message(EventMessage),
	}
}



decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {

		type Error = Error<T>;

		fn deposit_event() = default;

		// update the campaign status
	 	// 0 not started, 1 in progress, 2 success, 3 pause, 4 cancel, 5 fail
	 	// admin can set any status
	 	// owner can pause, cancel
		#[weight = 1_000]
		fn update_status(
			origin,
			campaign: T::Hash,
			status: u8
		) -> DispatchResult {
			// campaign exists?
			let sender = ensure_signed(origin)?;
			// read status
			let owner = Self::owner_of(campaign) .ok_or(Error::<T>::OwnerUnknown)?;
			ensure!( owner == sender, Error::<T>::AuthorizationError );
			// update status
			let status = Self::state_of(campaign); // ??? .ok_or(Error::<T>::GuruMeditation)?;
			// write status
			let now = <system::Module<T>>::block_number();
			// dispatch event
			Self::deposit_event(
				RawEvent::CampaignUpdated(
					campaign,
					status,
					now
				)
			);

			Ok(())
		}

		#[weight = 10_000]
		fn create(
			origin,
			name: Vec<u8>,
			target: T::Balance,
			deposit: T::Balance,
			// TODO: should be duration in days,
			// not target blocknumber
			expiry: T::BlockNumber,
		) {

			// get the creator
			let creator = ensure_signed(origin)?;

			// check name length boundary
			ensure!(
				name.len() >= T::MinLength::get(),
				Error::<T>::NameTooShort
			);
			ensure!(
				name.len() <= T::MaxLength::get(),
				Error::<T>::NameTooLong
			);

			// get the nonce to help generate unique id
			let nonce = T::Nonce::get();

			let now = <system::Module<T>>::block_number();

			// ensure campaign expires after now
			ensure!(
				expiry > now,
				Error::<T>::EndTooEarly
			);


			// TODO: refactor calculate dest. block
			// let blocktime = 5;
			// let target_block_number =

			// TODO: fix BlockNumber sa
			// ensure!(
			// 	expiry <= <system::Module<T>>::block_number() + Self::campaign_max_duration_limit(),
			// 	Error::<T>::EndTooLate
			// );

			// generate the unique campaign id
			let phrase = b"crowdfunding_campaign";
			let id = T::Randomness::random(phrase);

			// TODO: check for correct padding
			// let seed = <[u8; 32]>::decode(&mut TrailingZeroInput::new(seed.as_ref()))
			// 	.expect("input is padded with zeroes; qed");
			// let id = seed.clone();

			// ensure unique id
			// ensure!(
			// 	!<CampaignOwner<T>>::exists(&id),
			// 	Error::<T>::CampaignIdExists
			// );

			// ensure deposit <= target
			// ensure!(
			// 	deposit <= target,
			// 	Error::<T>::CampaignDepositTooHigh
			// );

			// check contribution limit per block
			// let contributions = Self::campaign_expire_at(expiry);
			// ensure!(
			// 	contributions.len() < MAX_CONTRIBUTIONS_PER_BLOCK,
			// 	Error::<T>::ContributionsPerBlockExceeded
			// );

			let protocol :u8 = 0;

			// create a new campaign
			// id: Hash,
			// name: Vec<u8>,
			// manager: AccountId,
			// deposit: Balance,
			// expiry: BlockNumber,
			// cap: Balance,
			// status: u8,
			let new_campaign = Campaign {
				id: id.clone(),
				// name: name,
				manager: creator.clone(),
				deposit: deposit.clone(),
				expiry: expiry,
				cap: target,
				name: name.clone(),
				protocol: protocol,
				status: 0,
			};

			// mint the campaign
			Self::mint(
				creator.clone(),
				id.clone(),
				expiry.clone(),
				deposit.clone(),
				protocol.clone(),
				new_campaign
			)?;

			// deposit the event
			Self::deposit_event(
				RawEvent::CampaignCreated(
					id,
					creator,
					target,
					deposit,
					expiry,
					name
				)
			);

			// No fees are paid here if we need to create this account;
			// that's why we don't just use the stock `transfer`.
			// T::Currency::resolve_creating(&Self::campaign_account_id(index), imb);

		}

		/// contribute to project
		#[weight = 10_000]
		fn contribute (
			origin,
			campaign: T::Hash,
			contribution: T::Balance
		) -> DispatchResult {

			let sender = ensure_signed(origin)?;

			// owners cannot contribute to their own campaign..
			let owner = Self::owner_of(campaign) .ok_or(Error::<T>::OwnerUnknown)?;
			ensure!( owner != sender, Error::<T>::NoContributionToOwnCampaign );

			// status: 0 not started, 1 in progress, 2 success, 3 fail
			// contribution only possible when state in progress..
			let state = Self::state_of(campaign);
			ensure!( state == 1, Error::<T>::NoContributionsAllowed);

			// submit
			Self::create_contribution(sender.clone(), campaign.clone(), contribution.clone())?;

			// get current blocktime
			let now = <system::Module<T>>::block_number();

			// send event
			Self::deposit_event(
				RawEvent::CampaignContributed(
					campaign,
					sender,
					contribution,
					now,
				)
			);

			Ok(())
		}

		/// finalize campaigns ending in current block
		fn on_finalize() {

			// get all the contributions of current block
			let block_number = <system::Module<T>>::block_number();
			let campaign_hashes = Self::campaign_expire_at(block_number);

			// iterate over hashes
			for campaign_id in &campaign_hashes {

				let mut campaign = Self::campaign_by_id(campaign_id);
				let total_contributions = Self::total_contributions(campaign_id);

				// check for cap reached
				if total_contributions >= campaign.cap {

					// update campaign state to success
					campaign.status = 2;
					<Campaigns<T>>::insert(campaign_id.clone(), campaign);

					// get campaign owner
					let _owner = Self::owner_of(campaign_id);

					match _owner {
						Some(owner) => {

							// get all contributors
							let contributors = Self::contributor_accounts(campaign_id);
							let mut transaction_complete = true;

							// 1 iterate over contributors
							// 2 unreserve contribution
							// 3 transfer contribution to campaign owner -> should be treasury!
							'inner: for contributor in &contributors {


								let contributor_balance = Self::contributed_amount((*campaign_id, contributor.clone()));
								let _ = <balances::Module<T>>::unreserve(&contributor, contributor_balance.clone());

								// if contributor == campaign owner
								// unreserve the money
								if contributor == &owner { continue; }
								let _transfer = <balances::Module<T> as Currency<_>>::transfer(
									&contributor,
									&owner,
									contributor_balance,
									ExistenceRequirement::AllowDeath
								);

								match _transfer {
									Err(_e) => {
										transaction_complete = false;
										break 'inner;
									},
									Ok(_v) => {}
								}

							}

							// If all transactions are settled
							// reserve all money of the funding
							if process_complete {
								let _ = <balances::Module<T>>::reserve(&owner, total_contributions);
								// deposit the event
								Self::deposit_event(RawEvent::CampaignFinalized(*campaign_id, total_contributions, block_number, true));
							}

						},
						None => continue,
					}

				// campaign cap not reached
				} else {

					// campaign failed
					// refund all of the money

					// update camapign state to failed
					// status: 0 not started, 1 in progress, 2 success, 3 fail
					campaign.status = 3;
					<Campaigns<T>>::insert(campaign_id.clone(), campaign);

					// revert all contributions
					let contributors = Self::contributor_accounts(campaign_id);
					for account in contributors {
						let contribution = Self::contributed_amount((*campaign_id, account.clone()));
						let _ = <balances::Module<T>>::unreserve(&account, contribution);
					}

					// deposit the event
					Self::deposit_event(RawEvent::CampaignFailed(*campaign_id, total_contributions, block_number, false));

				}
			}

		}

 	}
}

impl<T: Trait> Module<T> {

	fn mint(
		// campaign creator
		sender: T::AccountId,
		// generated campaign id
		id: T::Hash,
		// expiration blocktime
		// example: desired lifetime == 30 days
		// 30 days * 24h * 60m / 5s avg blocktime ==
		// 2592000s / 5s == 518400 blocks from now.
		expiry: T::BlockNumber,
		// campaign creator deposit to invoke the campaign
		deposit: T::Balance,
		// funding protocol
		// 0 grant, 1 prepaid, 2 loan, 3 shares, 4 dao
		// proper assignment of funds into the instrument
		// happens after successful funding of the campaing
		protocol: u8,
		// campaign object
		new_campaign: Campaign<T::Hash, T::AccountId, T::Balance, T::BlockNumber>
	) -> DispatchResult {

		// insert into campaigns map
		<Campaigns<T>>::insert(id.clone(), new_campaign.clone());

		// insert into campaign owners map
		<CampaignOwner<T>>::insert(id.clone(), sender.clone());

		// insert into campaigns by expiration block
		<CampaignsByBlockNumber<T>>::mutate(expiry.clone(), |campaigns| campaigns.push(id.clone()));

		// global campaigns count
		let all_campaigns_count = Self::all_campaigns_count();
		let update_all_campaigns_count = all_campaigns_count.checked_add(1).ok_or(Error::<T>::AddCampaignOverflow)?;

		// owned campaigns count
		let owned_campaigns_count = Self::owned_campaigns_count(&sender);
		let update_owned_campaigns_count = owned_campaigns_count.checked_add(1).ok_or(Error::<T>::AddContributionOverflow)?;

		// update global campaign count
		<AllCampaignsArray<T>>::insert(&all_campaigns_count, id.clone());
		<AllCampaignsCount>::put(update_all_campaigns_count);
		<AllCampaignsIndex<T>>::insert(id.clone(), all_campaigns_count);

		// update owned campaign count
		<OwnedCampaignsArray<T>>::insert((sender.clone(), owned_campaigns_count.clone()), id.clone());
		<OwnedCampaignsCount<T>>::insert(&sender, update_owned_campaigns_count);
		<OwnedCampaignsIndex<T>>::insert((sender.clone(), id.clone()), owned_campaigns_count);

		// write

		// TODO: final check
		// TODO: final check
		// if deposit exceeds available balance,
		// revert the campaign
		// if deposit > Balance::sa(0) {
		// 	match Self::create_contribution(sender.clone(), id.clone(), deposit.clone()){
		// 		Err(_e) => {
		// 			<Campaigns<T>>::remove(id.clone());
		// 			<CampaignOwner<T>>::remove(id.clone());
		// 			<CampaignsByBlockNumber<T>>::mutate(expiry,|campaigns| campaigns.pop());
		// 			<AllCampaignsArray<T>>::remove(&all_campaigns_count);
		// 			<AllCampaignsCount>::put(all_campaigns_count.clone());
		// 			<AllCampaignsIndex<T>>::remove(id.clone());
		// 			<OwnedCampaignsArray<T>>::remove((sender.clone(), owned_campaigns_count.clone()));
		// 			<OwnedCampaignsCount<T>>::remove(&sender);
		// 			<OwnedCampaignsIndex<T>>::remove((sender.clone(), id.clone()));
		// 		},
		// 		Ok(_v) => {}
		// 	}
		// }

		// nonce ++
		Nonce::mutate(|n| *n += 1);

		Ok(())
	}

	fn create_contribution(
		sender: T::AccountId,
		campaign_key: T::Hash,
		contribution: T::Balance
	) -> DispatchResult {

		// campaign exists ?
		ensure!( <Campaigns<T>>::contains_key(campaign_key), Error::<T>::InvalidId );
		let campaign = Self::campaign_by_id(&campaign_key);

		// campaign still active ?
		ensure!(<system::Module<T>>::block_number() < campaign.expiry, Error::<T>::CampaignExpired );

		// contributor has sufficient balance ?
		ensure!( <balances::Module<T>>::free_balance(sender.clone()) >= contribution, Error::<T>::BalanceTooLow );

		// meta data
		// check if contributor exists
		// if not, update metadata
		if !<ContributedAmount<T>>::contains_key((&campaign_key, &sender)) {

			// increase the number of campaigncontributors invested in
			let contributed_campaigns = Self::contributed_campaigns_count(&sender);
			let update_contributed_campaigns = contributed_campaigns.checked_add(1).ok_or(Error::<T>::AddContributionOverflow)?;

			// increase the number of contributors into the campaign
			let contributors = <ContributorAccountsCount<T>>::get(&campaign_key);
			let update_contributors = contributors.checked_add(1).ok_or(Error::<T>::UpdateContributorOverflow)?;

			// change the state of invest related fields
			// <ContributedAmount<T>>::insert((campaign_key.clone(), sender.clone()), contribution.clone());
			<ContributorAccounts<T>>::mutate(&campaign_key, |accounts| accounts.push(sender.clone()));

			// update total contributor count
			<ContributorAccountsCount<T>>::insert(campaign_key.clone(), update_contributors);

			// update contributed campaigns
			<ContributedCampaignsArray<T>>::insert((sender.clone(), contributed_campaigns), campaign_key);
			<ContributedCampaignsCount<T>>::insert(&sender, update_contributed_campaigns);
			<ContributedCampaignsIndex<T>>::insert((sender.clone(), campaign_key.clone()), contributed_campaigns);

		}

		// reserve
		<balances::Module<T>>::reserve(&sender, contribution)?;

		// update contributions to campaign per contributor
		let total_contributions = Self::contributed_amount((&campaign_key, &sender));
		let update_total_contributions = total_contributions + contribution;
		<ContributedAmount<T>>::insert((&campaign_key, &sender), update_total_contributions);

		// update total contributions to campaign
		let total_campaign_contributions = Self::total_contributions(&campaign_key);
		let update_campaign_contributions = total_campaign_contributions + contribution;
		<TotalContributions<T>>::insert(&campaign_key, update_campaign_contributions);

		Ok(())
	}

}

//
//
//
//
//

decl_error! {
	pub enum Error for Module<T: Trait> {

		//
		//	general
		//
		/// Must contribute at least the minimum amount of Campaigns
		ContributionTooSmall,
		/// Balance too low.
		BalanceTooLow,
		/// The Campaign id specified does not exist
		InvalidId,
		/// The Campaign's contribution period has ended; no more contributions will be accepted
		ContributionPeriodOver,
		/// You may not withdraw or dispense Campaigns while the Campaign is still active
		CampaignStillActive,
		/// You cannot withdraw Campaigns because you have not contributed any
		NoContribution,
		/// You cannot dissolve a Campaign that has not yet completed its retirement period
		CampaignNotRetired,
		/// Campaign expired
		CampaignExpired,
		/// Cannot dispense Campaigns from an unsuccessful Campaign
		UnsuccessfulCampaign,

		//
		//	create
		//
		/// Campaign must end after it starts
		EndTooEarly,
		/// Campaign expiry has be lower than the block number limit
		EndTooLate,
		/// Max contributions per block exceeded
		ContributionsPerBlockExceeded,
		/// Name too long
		NameTooLong,
		/// Name too short
		NameTooShort,
		/// Deposit exceeds the campaign target
		DepositTooHigh,
		/// Campaign id exists
		IdExists,

		//
		//	mint
		//
		/// Overflow adding a new campaign to total fundings
		AddCampaignOverflow,
		/// Overflow adding a new contribution to account balance
		AddContributionOverflow,
		/// Overflow adding to the total number of contributors of a camapaign
		UpdateContributorOverflow,
		/// Campaign owner unknown
		OwnerUnknown,
		/// Cannot contribute to owned campaign
		NoContributionToOwnCampaign,
		/// Guru Meditation
		GuruMeditation,
		/// Zou are not authorized for this call
		AuthorizationError,
		/// Contributions not allowed
		NoContributionsAllowed,

	}
}
