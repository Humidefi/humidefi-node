#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{
	sp_runtime::{
		traits::{AccountIdConversion, EnsureAdd, EnsureDiv, EnsureMul, EnsureSub, Zero},
		ArithmeticError, FixedU128, Perbill,
	},
	traits::{fungible, fungibles},
	PalletId,
};

const DEX_PALLET: PalletId = PalletId(*b"HUMIDEFI");

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		type Fungibles: fungibles::Inspect<Self::AccountId, AssetId = u32, Balance = u128>
			+ fungibles::Mutate<Self::AccountId>
			+ fungibles::Create<Self::AccountId>;
	}

	pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	pub type AssetIdOf<T> = <<T as Config>::Fungibles as fungibles::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::AssetId;

	pub type AssetBalanceOf<T> = <<T as Config>::Fungibles as fungibles::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	pub type AccountLiquidityPoolId = u64;

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AssetPairs<T: Config> {
		pub asset_x: AssetIdOf<T>,
		pub asset_y: AssetIdOf<T>,
	}

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct LiquidityPool<T: Config> {
		pub asset_pair: AssetPairs<T>,
		pub asset_x_balance: FixedU128,
		pub asset_y_balance: FixedU128,
		pub price: FixedU128,
		pub asset_x_fee: FixedU128,
		pub asset_y_fee: FixedU128,
		pub lp_token: AssetIdOf<T>,
		pub lp_token_balance: FixedU128,
	}

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AccountLiquidityPool<T: Config> {
		pub id: AccountLiquidityPoolId,
		pub account_id: <T as frame_system::Config>::AccountId,
		pub asset_pair: AssetPairs<T>,
		pub asset_x_balance: FixedU128,
		pub asset_y_balance: FixedU128,
		pub lp_token: AssetIdOf<T>,
		pub lp_token_balance: FixedU128,
	}

	#[pallet::storage]
	#[pallet::getter(fn liquidity_pool_storage)]
	pub type LiquidityPoolStorage<T> =
		StorageMap<_, Blake2_128Concat, AssetPairs<T>, LiquidityPool<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn account_liquidity_pool_storage)]
	pub type AccountLiquidityPoolStorage<T> = StorageMap<
		_,
		Blake2_128Concat,
		(<T as frame_system::Config>::AccountId, AssetPairs<T>),
		BoundedVec<AccountLiquidityPool<T>, ConstU32<100>>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		LiquidityAddedSuccessfully { account_liquidity_pool: AccountLiquidityPool<T> },
		LiquidityRedeemedSuccessfully,
		SwapExecutedSuccessfully,
		TransferExecutedSuccessfully
	}

	#[pallet::error]
	pub enum Error<T> {
		CheckAssetXBalanceError,
		CheckAssetYBalanceError,
		CheckAssetLiquidityPoolTokenBalanceError,
		CheckAssetSwapInBalanceError,
		CheckAssetSwapOutBalanceError,
		AssetDoesNotHaveEnoughBalance,

		ComputeAndMintLiquidityPoolTokenError,
		ComputePriceError,
		ComputeXYBalancesError,
		CannotBeZero,

		LiquidityPoolDoesNotExists,

		AccountLiquidityPoolBoundedVecError,
		AccountLiquidityPoolIdError,
		AccountLiquidityPoolDoesNotExists,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn add_liquidity(
			origin: OriginFor<T>,
			asset_pair: AssetPairs<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y_balance: AssetBalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();

			Self::check_asset_balance(
				who.clone(),
				asset_pair.clone().asset_x,
				asset_x_balance,
			).expect(Error::<T>::CheckAssetXBalanceError.into());

			Self::check_asset_balance(
				who.clone(),
				asset_pair.clone().asset_y,
				asset_x_balance,
			).expect(Error::<T>::CheckAssetYBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_pair.clone().asset_x,
				&who.clone(),
				&dex_account_id.clone(),
				asset_x_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_pair.clone().asset_y,
				&who.clone(),
				&dex_account_id.clone(),
				asset_y_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let mint_liquidity = Self::compute_and_mint_lp_token(
				asset_pair.clone(),
				asset_x_balance,
				asset_y_balance,
			).expect(Error::<T>::ComputeAndMintLiquidityPoolTokenError.into());

			let lp_token = mint_liquidity.0;
			let lp_token_balance = mint_liquidity.1;

			Self::check_asset_balance(
				dex_account_id.clone(),
				lp_token,
				lp_token_balance,
			).expect(Error::<T>::CheckAssetLiquidityPoolTokenBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				lp_token.clone(),
				&dex_account_id.clone(),
				&who.clone(),
				lp_token_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let get_liquidity_pool = Self::get_liquidity_pool(asset_pair.clone());
			match get_liquidity_pool {
				Some(liquidity_pool) => {
					let update_asset_x_balance = liquidity_pool
						.asset_x_balance
						.add(FixedU128::from_inner(asset_x_balance));

					let update_asset_y_balance = liquidity_pool
						.asset_y_balance
						.add(FixedU128::from_inner(asset_y_balance));

					let update_price = Self::compute_price(
						update_asset_x_balance.into_inner(), 
						update_asset_y_balance.into_inner()
					).expect(Error::<T>::ComputePriceError.into());

					let update_lp_token_balance = liquidity_pool
						.lp_token_balance
						.add(FixedU128::from_inner(lp_token_balance));

					LiquidityPoolStorage::<T>::mutate(asset_pair.clone(), |query| {
						let liquidity_pool_payload = LiquidityPool::<T> {
							asset_pair: asset_pair.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							price: update_price,
							asset_x_fee: liquidity_pool.asset_x_fee,
							asset_y_fee: liquidity_pool.asset_y_fee,
							lp_token: liquidity_pool.lp_token,
							lp_token_balance: update_lp_token_balance,
						};

						*query = Some(liquidity_pool_payload);
					});
				},
				None => {
					let new_price = Self::compute_price(
						asset_x_balance,
						asset_y_balance
					).expect(Error::<T>::ComputePriceError.into());
	
					let liquidity_pool_payload = LiquidityPool::<T> {
						asset_pair: asset_pair.clone(),
						asset_x_balance: FixedU128::from_inner(asset_x_balance),
						asset_y_balance: FixedU128::from_inner(asset_y_balance),
						price: new_price,
						asset_x_fee: FixedU128::from_inner(0),
						asset_y_fee: FixedU128::from_inner(0),
						lp_token,
						lp_token_balance: FixedU128::from_inner(lp_token_balance),
					};
	
					LiquidityPoolStorage::<T>::insert(asset_pair.clone(), liquidity_pool_payload);
				}
			}

			let mut account_liquidity_pool_payload = AccountLiquidityPool::<T> {
				id: 1u64.into(),
				account_id: who.clone(),
				asset_pair: asset_pair.clone(),
				asset_x_balance: FixedU128::from_inner(asset_x_balance),
				asset_y_balance: FixedU128::from_inner(asset_y_balance),
				lp_token,
				lp_token_balance: FixedU128::from_inner(lp_token_balance),
			};

			let get_account_liquidity_pools = Self::get_account_liquidity_pools(who.clone(), asset_pair.clone());
			match get_account_liquidity_pools {
				Some(account_liquidity_pools) => {
					let mut last_id = 0u64.into();
					if let Some(account_liquidity_pool) = account_liquidity_pools.last() {
						last_id = account_liquidity_pool.id;
					}

					account_liquidity_pool_payload.id = last_id
						.ensure_add(1)
						.expect(Error::<T>::AccountLiquidityPoolIdError.into());

					let mut mutate_account_liquidity_pools = account_liquidity_pools.clone();
					mutate_account_liquidity_pools
						.try_push(account_liquidity_pool_payload.clone())
						.expect(Error::<T>::AccountLiquidityPoolBoundedVecError.into());

					let storage_key = (who.clone(), asset_pair.clone());
					AccountLiquidityPoolStorage::<T>::mutate(storage_key, |query| {
						let update_account_liquidity_pools = mutate_account_liquidity_pools.clone();
						*query = Some(update_account_liquidity_pools)
					});
				},
				None => {
					let mut new_account_liquidity_pools: BoundedVec<
						AccountLiquidityPool<T>,
						ConstU32<100>,
					> = Default::default();

					new_account_liquidity_pools
						.try_push(account_liquidity_pool_payload.clone())
						.expect(Error::<T>::AccountLiquidityPoolBoundedVecError.into());

					AccountLiquidityPoolStorage::<T>::insert(
						(who, asset_pair),
						new_account_liquidity_pools,
					);
				}
			}

			Self::deposit_event(Event::LiquidityAddedSuccessfully {
				account_liquidity_pool: account_liquidity_pool_payload,
			});

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn redeem_liquidity(
			origin: OriginFor<T>,
			asset_pair: AssetPairs<T>,
			lp_token: AssetIdOf<T>,
			id: AccountLiquidityPoolId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let get_liquidity_pool = Self::get_liquidity_pool(asset_pair.clone());
			if !get_liquidity_pool.is_some() {
				return Err(Error::<T>::LiquidityPoolDoesNotExists.into())
			}

			let asset_xy_balances = Self::compute_xy_assets(
				who.clone(), 
				asset_pair.clone(), 
				lp_token, 
				id
			).expect(Error::<T>::ComputeXYBalancesError.into());

			let asset_x_balance = asset_xy_balances.0;
			let asset_y_balance = asset_xy_balances.1;
			let lp_token_balance = asset_xy_balances.2;

			let dex_account_id = Self::get_dex_account();

			Self::check_asset_balance(
				dex_account_id.clone(),
				asset_pair.clone().asset_x,
				asset_x_balance,
			).expect(Error::<T>::CheckAssetXBalanceError.into());

			Self::check_asset_balance(
				dex_account_id.clone(),
				asset_pair.clone().asset_x,
				asset_y_balance,
			).expect(Error::<T>::CheckAssetYBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_pair.clone().asset_x,
				&dex_account_id.clone(),
				&who.clone(),
				asset_x_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_pair.clone().asset_y,
				&dex_account_id.clone(),
				&who.clone(),
				asset_y_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			LiquidityPoolStorage::<T>::mutate(asset_pair.clone(), |query| {
				if let Some(mutate_liquidity_pool) = query {
					let update_asset_x_balance = mutate_liquidity_pool
						.asset_x_balance
						.sub(FixedU128::from_inner(asset_x_balance));

					let update_asset_y_balance = mutate_liquidity_pool
						.asset_y_balance
						.sub(FixedU128::from_inner(asset_y_balance));

					let update_price = Self::compute_price(
						update_asset_x_balance.into_inner(), 
						update_asset_y_balance.into_inner()
					).expect(Error::<T>::ComputePriceError.into());

					let update_lp_token_balance = mutate_liquidity_pool
						.lp_token_balance
						.sub(FixedU128::from_inner(lp_token_balance));

					let liquidity_pool_payload = LiquidityPool::<T> {
						asset_pair: asset_pair.clone(),
						asset_x_balance: update_asset_x_balance,
						asset_y_balance: update_asset_y_balance,
						price: update_price,
						asset_x_fee: mutate_liquidity_pool.asset_x_fee,
						asset_y_fee: mutate_liquidity_pool.asset_y_fee,
						lp_token: mutate_liquidity_pool.lp_token,
						lp_token_balance: update_lp_token_balance,
					};

					*query = Some(liquidity_pool_payload);
				}
			});

			AccountLiquidityPoolStorage::<T>::remove((who.clone(), asset_pair.clone()));

			Self::deposit_event(Event::LiquidityRedeemedSuccessfully);

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn swap_exact_in_for_out(
			origin: OriginFor<T>,
			asset_exact_in: AssetIdOf<T>,
			asset_exact_in_balance: AssetBalanceOf<T>,
			asset_max_out: AssetIdOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			
			Self::check_asset_balance(
				who.clone(),
				asset_exact_in,
				asset_exact_in_balance,
			).expect(Error::<T>::CheckAssetSwapInBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_exact_in,
				&who.clone(),
				&dex_account_id.clone(),
				asset_exact_in_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let asset_pair = AssetPairs::<T> { asset_x: asset_exact_in, asset_y: asset_max_out };
			let get_liquidity_pool = Self::get_liquidity_pool(asset_pair.clone());
			match get_liquidity_pool {
				Some(liquidity_pool) => {
					let mut price = FixedU128::from_inner(0);

					if asset_exact_in == liquidity_pool.asset_pair.asset_x {
						price = Self::compute_price(
							liquidity_pool.asset_x_balance.into_inner(),
							liquidity_pool.asset_y_balance.into_inner()
						).expect(Error::<T>::ComputePriceError.into());
					}

					if asset_exact_in == liquidity_pool.asset_pair.asset_y {
						price = Self::compute_price(
							liquidity_pool.asset_y_balance.into_inner(),
							liquidity_pool.asset_x_balance.into_inner()
						).expect(Error::<T>::ComputePriceError.into());
					}

					let asset_max_out_balance = FixedU128::from_inner(price.into_inner()).mul(FixedU128::from_inner(asset_exact_in_balance));

					Self::check_asset_balance(
						dex_account_id.clone(),
						asset_max_out,
						asset_max_out_balance.into_inner(),
					).expect(Error::<T>::CheckAssetSwapOutBalanceError.into());
		
					<T::Fungibles as fungibles::Mutate<_>>::transfer(
						asset_max_out,
						&dex_account_id.clone(),
						&who.clone(),
						asset_max_out_balance.into_inner(),
						frame_support::traits::tokens::Preservation::Expendable,
					)?;

					let mut update_asset_x_balance = FixedU128::from_inner(0);
					let mut update_asset_y_balance = FixedU128::from_inner(0);

					if asset_exact_in == liquidity_pool.asset_pair.asset_x {
						update_asset_x_balance = liquidity_pool
							.asset_x_balance
							.add(FixedU128::from_inner(asset_exact_in_balance));

						update_asset_y_balance = liquidity_pool
							.asset_y_balance
							.sub(asset_max_out_balance);
					}

					if asset_exact_in == liquidity_pool.asset_pair.asset_y {
						update_asset_x_balance = liquidity_pool
							.asset_x_balance
							.sub(asset_max_out_balance);

						update_asset_y_balance = liquidity_pool
							.asset_y_balance
							.add(FixedU128::from_inner(asset_exact_in_balance));
					}

					let update_price = Self::compute_price(
						update_asset_x_balance.into_inner(), 
						update_asset_y_balance.into_inner()
					).expect(Error::<T>::ComputePriceError.into());

					LiquidityPoolStorage::<T>::mutate(asset_pair.clone(), |query| {
						let liquidity_pool_payload = LiquidityPool::<T> {
							asset_pair: asset_pair.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							price: update_price,
							asset_x_fee: liquidity_pool.asset_x_fee,
							asset_y_fee: liquidity_pool.asset_y_fee,
							lp_token: liquidity_pool.lp_token,
							lp_token_balance: liquidity_pool.lp_token_balance,
						};

						*query = Some(liquidity_pool_payload);
					});
				},
				None => {
					return Err(Error::<T>::LiquidityPoolDoesNotExists.into())
				}
			}

			Self::deposit_event(Event::SwapExecutedSuccessfully);

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn swap_in_for_exact_out(
			origin: OriginFor<T>,
			asset_exact_out: AssetIdOf<T>,
			asset_exact_out_balance: AssetBalanceOf<T>,
			asset_min_in: AssetIdOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			
			Self::check_asset_balance(
				dex_account_id.clone(),
				asset_exact_out,
				asset_exact_out_balance,
			).expect(Error::<T>::CheckAssetSwapOutBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_exact_out,
				&dex_account_id.clone(),
				&who.clone(),
				asset_exact_out_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let asset_pair = AssetPairs::<T> { asset_x: asset_min_in, asset_y: asset_exact_out };
			let get_liquidity_pool = Self::get_liquidity_pool(asset_pair.clone());
			match get_liquidity_pool {
				Some(liquidity_pool) => {
					let mut price = FixedU128::from_inner(0);

					if asset_min_in == liquidity_pool.asset_pair.asset_x {
						price = Self::compute_price(
							liquidity_pool.asset_x_balance.into_inner(),
							liquidity_pool.asset_y_balance.into_inner()
						).expect(Error::<T>::ComputePriceError.into());
					}

					if asset_min_in == liquidity_pool.asset_pair.asset_y {
						price = Self::compute_price(
							liquidity_pool.asset_y_balance.into_inner(),
							liquidity_pool.asset_x_balance.into_inner()
						).expect(Error::<T>::ComputePriceError.into());
					}

					let asset_min_in_balance = FixedU128::from_inner(price.into_inner()).mul(FixedU128::from_inner(asset_exact_out_balance));

					Self::check_asset_balance(
						who.clone(),
						asset_min_in,
						asset_min_in_balance.into_inner(),
					).expect(Error::<T>::CheckAssetSwapInBalanceError.into());
		
					<T::Fungibles as fungibles::Mutate<_>>::transfer(
						asset_min_in,
						&who.clone(),
						&dex_account_id.clone(),
						asset_min_in_balance.into_inner(),
						frame_support::traits::tokens::Preservation::Expendable,
					)?;

					let mut update_asset_x_balance = FixedU128::from_inner(0);
					let mut update_asset_y_balance = FixedU128::from_inner(0);

					if asset_min_in == liquidity_pool.asset_pair.asset_x {
						update_asset_x_balance = liquidity_pool
							.asset_x_balance
							.add(asset_min_in_balance);

						update_asset_y_balance = liquidity_pool
							.asset_y_balance
							.sub(FixedU128::from_inner(asset_exact_out_balance));
					}

					if asset_min_in == liquidity_pool.asset_pair.asset_y {
						update_asset_x_balance = liquidity_pool
							.asset_x_balance
							.sub(FixedU128::from_inner(asset_exact_out_balance));

						update_asset_y_balance = liquidity_pool
							.asset_y_balance
							.add(asset_min_in_balance);
					}

					let update_price = Self::compute_price(
						update_asset_x_balance.into_inner(), 
						update_asset_y_balance.into_inner()
					).expect(Error::<T>::ComputePriceError.into());

					LiquidityPoolStorage::<T>::mutate(asset_pair.clone(), |query| {
						let liquidity_pool_payload = LiquidityPool::<T> {
							asset_pair: asset_pair.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							price: update_price,
							asset_x_fee: liquidity_pool.asset_x_fee,
							asset_y_fee: liquidity_pool.asset_y_fee,
							lp_token: liquidity_pool.lp_token,
							lp_token_balance: liquidity_pool.lp_token_balance,
						};

						*query = Some(liquidity_pool_payload);
					});
				},
				None => {
					return Err(Error::<T>::LiquidityPoolDoesNotExists.into())
				}
			}

			Self::deposit_event(Event::SwapExecutedSuccessfully);

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		pub fn transfer_asset(
			origin: OriginFor<T>,
			asset: AssetIdOf<T>,
			asset_balance: AssetBalanceOf<T>,
			account_id: <T as frame_system::Config>::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Self::check_asset_balance(
				who.clone(),
				asset,
				asset_balance,
			).expect(Error::<T>::CheckAssetSwapInBalanceError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset,
				&who.clone(),
				&account_id.clone(),
				asset_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			Self::deposit_event(Event::TransferExecutedSuccessfully);

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn get_dex_account() -> T::AccountId {
			DEX_PALLET.into_account_truncating()
		}

		pub fn get_asset_balance(
			asset: AssetIdOf<T>,
			account_id: <T as frame_system::Config>::AccountId,
		) -> AssetBalanceOf<T> {
			let balance = <T::Fungibles as fungibles::Inspect<_>>::balance(asset, &account_id);
			balance
		}

		pub fn get_liquidity_pool(
			asset_pair: AssetPairs<T>
		) -> Option<LiquidityPool<T>> {
			let existing_liquidity_pool = LiquidityPoolStorage::<T>::get(asset_pair.clone());
			match existing_liquidity_pool {
				Some(liquidity_pool) => return Some(liquidity_pool),
				None => {
					let swap_asset_pair = AssetPairs::<T> {
						asset_x: asset_pair.clone().asset_y,
						asset_y: asset_pair.clone().asset_x,
					};

					let liquidity_pool_swap_pair = LiquidityPoolStorage::<T>::get(swap_asset_pair);
					if let Some(liquidity_pool) = liquidity_pool_swap_pair {
						return Some(liquidity_pool)
					}

					return None
				},
			}
		}

		pub fn get_account_liquidity_pools(
			account_id: <T as frame_system::Config>::AccountId,
			asset_pair: AssetPairs<T>,
		) -> Option<BoundedVec<AccountLiquidityPool<T>, ConstU32<100>>> {
			let storage_key = (account_id.clone(), asset_pair.clone());
			let existing_account_liquidity_pools = AccountLiquidityPoolStorage::<T>::get(storage_key);
			match existing_account_liquidity_pools {
				Some(account_liquidity_pools) => return Some(account_liquidity_pools),
				None => {
					let swap_asset_pair = AssetPairs::<T> {
						asset_x: asset_pair.clone().asset_y,
						asset_y: asset_pair.clone().asset_x,
					};

					let storage_key_swap_pair = (account_id.clone(), swap_asset_pair.clone());
					let account_liquidity_pools_swap_pair =
						AccountLiquidityPoolStorage::<T>::get(storage_key_swap_pair);

					if let Some(account_liquidity_pool) = account_liquidity_pools_swap_pair {
						return Some(account_liquidity_pool)
					}

					return None
				},
			}
		}

		pub fn check_asset_balance(
			account_id: <T as frame_system::Config>::AccountId,
			asset: AssetIdOf<T>,
			asset_balance: AssetBalanceOf<T>,
		) -> Result<(), DispatchError> {
			let current_asset_balance = Self::get_asset_balance(asset, account_id.clone());

			current_asset_balance
				.ensure_sub(asset_balance)
				.expect(Error::<T>::AssetDoesNotHaveEnoughBalance.into());

			Ok(())
		}

		pub fn compute_and_mint_lp_token(
			asset_pair: AssetPairs<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y_balance: AssetBalanceOf<T>,
		) -> Result<(AssetIdOf<T>, AssetBalanceOf<T>), DispatchError> {
			let mut lp_token: AssetIdOf<T> = 1u32.into();
			let dex_account_id = Self::get_dex_account();

			let existing_liquidity_pool = Self::get_liquidity_pool(asset_pair.clone());

			match existing_liquidity_pool {
				Some(liquidity_pool) => {
					lp_token = liquidity_pool.lp_token;
				},
				None => {
					let mut counter = 1u32.into();

					loop {
						lp_token = counter;

						if !<T::Fungibles as fungibles::Inspect<_>>::asset_exists(lp_token) {
							<T::Fungibles as fungibles::Create<_>>::create(
								lp_token,
								dex_account_id.clone(),
								true,
								1u128.into(),
							)?;

							break;
						}

						counter += 1;
					}
				},
			}

			let mul_xy_assets = FixedU128::from_inner(asset_x_balance).mul(FixedU128::from_inner(asset_y_balance));
			if mul_xy_assets.is_zero() {
				return Err(Error::<T>::CannotBeZero.into())
			}

			let lp_token_balance = mul_xy_assets.sqrt().into_inner();

			<T::Fungibles as fungibles::Mutate<_>>::mint_into(
				lp_token,
				&dex_account_id.clone(),
				lp_token_balance,
			)?;

			Ok((lp_token, lp_token_balance))
		}

		pub fn compute_price(
			asset_x_balance: AssetBalanceOf<T>,
			asset_y_balance: AssetBalanceOf<T>,
		) -> Result<FixedU128, DispatchError> {
			if FixedU128::from_inner(asset_x_balance).is_zero() || FixedU128::from_inner(asset_y_balance).is_zero() {
				return Err(Error::<T>::CannotBeZero.into())
			}

			let price = FixedU128::from_rational(asset_y_balance, asset_x_balance);
			Ok(price)
		}

		pub fn compute_xy_assets(
			account_id: <T as frame_system::Config>::AccountId,
			asset_pair: AssetPairs<T>,
			lp_token: AssetIdOf<T>,
			id: AccountLiquidityPoolId,
		) -> Result<(AssetBalanceOf<T>, AssetBalanceOf<T>, AssetBalanceOf<T>), DispatchError> {
			let existing_account_liquidity_pools = Self::get_account_liquidity_pools(account_id.clone(), asset_pair.clone());
			if !existing_account_liquidity_pools.is_some() {
				return Err(Error::<T>::AccountLiquidityPoolDoesNotExists.into())
			} 

			let mut lp_token_balance = FixedU128::from_inner(0);
			if let Some(account_liquidity_pools) = existing_account_liquidity_pools {
				if account_liquidity_pools.to_vec().len() > 0 {
					for account_liquidity_pool in account_liquidity_pools {
						if account_liquidity_pool.lp_token == lp_token && account_liquidity_pool.id == id {
							lp_token_balance = account_liquidity_pool.lp_token_balance;
							break;
						}
					}
				}
			};

			let mut price = FixedU128::from_inner(0);
			let existing_liquidity_pool = LiquidityPoolStorage::<T>::get(asset_pair.clone());
			if let Some(liquidity_pool) = existing_liquidity_pool {
				price = liquidity_pool.price;
			}
			
			if price.is_zero() {
				return Err(Error::<T>::CannotBeZero.into())
			}

			let get_asset_x_balance = lp_token_balance.div(FixedU128::from_inner(price.into_inner()).sqrt());
			let get_asset_y_balance = lp_token_balance.mul(FixedU128::from_inner(price.into_inner()).sqrt());
			let get_lp_token_balance = lp_token_balance;

			Ok((get_asset_x_balance.into_inner(), get_asset_y_balance.into_inner(), get_lp_token_balance.into_inner()))
		}
	}
}
