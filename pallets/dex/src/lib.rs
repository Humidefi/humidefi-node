#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{
	sp_runtime::{
		traits::{
			AccountIdConversion, EnsureAdd, EnsureDiv, EnsureMul, EnsureSub, Zero
		},
		ArithmeticError, FixedU128, Perbill,
	},
	traits::{fungible, fungibles},
	PalletId
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

	pub type AccountLiquidityPoolIndex = u32;

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AssetPairs<T: Config> {
		pub asset_x: AssetIdOf<T>,
		pub asset_y: AssetIdOf<T>,
	}

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct LiquidityPool<T: Config> {
		pub asset_pairs: AssetPairs<T>,
		pub asset_x_balance: FixedU128,
		pub asset_y_balance: FixedU128,
		pub lp_token: AssetIdOf<T>,
	}

	#[derive(Clone, Eq, PartialEq, DebugNoBound, TypeInfo, Encode, Decode, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AccountLiquidityPool<T: Config> {
		pub index: AccountLiquidityPoolIndex,
		pub account_id: <T as frame_system::Config>::AccountId,
		pub asset_pairs: AssetPairs<T>,
		pub asset_x_balance: FixedU128,
		pub asset_y_balance: FixedU128,
		pub price_ratio: FixedU128,
		pub lp_token: AssetIdOf<T>,
		pub lp_token_balance: FixedU128,
	}

	#[pallet::storage]
	#[pallet::getter(fn liquidity_pool_storage)]
	pub type LiquidityPoolStorage<T> = StorageMap<_, Blake2_128Concat, AssetPairs<T>, LiquidityPool<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn liquidity_pool_ledger_storage)]
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
		LiquidityPoolCreatedSuccessfully { account_liquidity_pool: AccountLiquidityPool<T> },
		LiquidityPoolProvidedSuccessfully { account_liquidity_pool: AccountLiquidityPool<T> },
		LiquidityPoolRedeemedSuccessfully,
	}

	#[pallet::error]
	pub enum Error<T> {
		CheckAssetBalancesError,
		AssetXDoesNotHaveEnoughBalance,
		AssetYDoesNotHaveEnoughBalance,

		ComputePriceRatioError,
		ComputePriceRatioArithmeticError,

		ComputeAndMintLiquidityPoolTokenError,
		ComputeAndMintLiquidityPoolTokenArithmeticError,

		LiquidityPoolError,
		LiquidityPoolAlreadyExists,
		LiquidityPoolDoesNotExists,

		CheckLiquidityPoolTokenBalanceError,
		LiquidityPoolTokenDoesNotHaveEnoughBalance,

		AccountLiquidityPoolError,
		AccountLiquidityPoolDoesNotExists,
		AccountLiquidityBoundedVecError,
		AccountLiquidityIndexArithmeticError,

		RedeemLiquidityPoolBalancesError,

		CheckAssetInAndOutBalancesError,
		AssetInDoesNotHaveEnoughBalance,
		AssetOutDoesNotHaveEnoughBalance,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn create_liquidity_pool(
			origin: OriginFor<T>,
			asset_x: AssetIdOf<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y: AssetIdOf<T>,
			asset_y_balance: AssetBalanceOf<T>
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			let asset_pairs = AssetPairs::<T> { asset_x, asset_y };

			let existing_liquidity_pool = Self::get_liquidity_pool(asset_pairs.clone());
			if existing_liquidity_pool.is_ok() {
				return Err(Error::<T>::LiquidityPoolAlreadyExists.into())
			}

			Self::check_xy_assets_balances(
				who.clone(),
				asset_pairs.clone(),
				asset_x_balance,
				asset_y_balance,
			).expect(Error::<T>::CheckAssetBalancesError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_x,
				&who.clone(),
				&dex_account_id.clone(),
				asset_x_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_y,
				&who.clone(),
				&dex_account_id.clone(),
				asset_y_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let minted_liquidity = Self::compute_and_mint_lp_token(
				asset_pairs.clone(),
				asset_x_balance,
				asset_y_balance
			).expect(Error::<T>::ComputeAndMintLiquidityPoolTokenError.into());

			let lp_token = minted_liquidity.0;
			let lp_token_balance = minted_liquidity.1;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				lp_token,
				&dex_account_id.clone(),
				&who.clone(),
				lp_token_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			LiquidityPoolStorage::<T>::insert(
				asset_pairs.clone(),
				LiquidityPool::<T> {
					asset_pairs: asset_pairs.clone(),
					asset_x_balance: FixedU128::from_inner(asset_x_balance),
					asset_y_balance: FixedU128::from_inner(asset_y_balance),
					lp_token: lp_token,
				},
			);

			let price_ratio = Self::compute_and_get_latest_price(
				asset_pairs.clone()
			).expect(Error::<T>::ComputePriceRatioError.into());

			let mut bounded_vec_account_liquidity_pools: BoundedVec<AccountLiquidityPool<T>, ConstU32<100>> = Default::default();
			let new_account_liquidity_pool = AccountLiquidityPool::<T> {
				index: 1u32.into(),
				account_id: who.clone(),
				asset_pairs: asset_pairs.clone(),
				asset_x_balance: FixedU128::from_inner(asset_x_balance),
				asset_y_balance: FixedU128::from_inner(asset_y_balance),
				price_ratio: FixedU128::from_inner(price_ratio),
				lp_token: lp_token,
				lp_token_balance: FixedU128::from_inner(lp_token_balance)
			};

			bounded_vec_account_liquidity_pools.try_push(new_account_liquidity_pool.clone()).expect(Error::<T>::AccountLiquidityBoundedVecError.into());

			AccountLiquidityPoolStorage::<T>::insert(
				(who, asset_pairs),
				bounded_vec_account_liquidity_pools,
			);

			Self::deposit_event(Event::LiquidityPoolCreatedSuccessfully {
				account_liquidity_pool: new_account_liquidity_pool
			});

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn provide_liquidity(
			origin: OriginFor<T>,
			asset_x: AssetIdOf<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y: AssetIdOf<T>,
			asset_y_balance: AssetBalanceOf<T>
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			let asset_pairs = AssetPairs::<T> { asset_x, asset_y };

			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			).expect(Error::<T>::LiquidityPoolError.into());

			Self::check_xy_assets_balances(
				who.clone(),
				asset_pairs.clone(),
				asset_x_balance,
				asset_y_balance,
			).expect(Error::<T>::CheckAssetBalancesError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_x,
				&who.clone(),
				&dex_account_id.clone(),
				asset_x_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_y,
				&who.clone(),
				&dex_account_id.clone(),
				asset_y_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let minted_liquidity = Self::compute_and_mint_lp_token(
				asset_pairs.clone(),
				asset_x_balance,
				asset_y_balance
			).expect(Error::<T>::ComputeAndMintLiquidityPoolTokenError.into());

			let lp_token = minted_liquidity.0;
			let lp_token_balance = minted_liquidity.1;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				lp_token,
				&dex_account_id.clone(),
				&who.clone(),
				lp_token_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let update_asset_x_balance = existing_liquidity_pool.asset_x_balance.add(FixedU128::from_inner(asset_x_balance));
			let update_asset_y_balance = existing_liquidity_pool.asset_y_balance.add(FixedU128::from_inner(asset_y_balance));

			LiquidityPoolStorage::<T>::mutate(
				asset_pairs.clone(), |query| {
					*query = Some(
						LiquidityPool::<T> {
							asset_pairs: asset_pairs.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							lp_token: lp_token,
						}
					);
				}
			);

			let price_ratio = Self::compute_and_get_latest_price(
				asset_pairs.clone()
			).expect(Error::<T>::ComputePriceRatioError.into());

			let mut new_account_liquidity_pool = AccountLiquidityPool::<T> {
				index: 0u32.into(),
				account_id: who.clone(),
				asset_pairs: asset_pairs.clone(),
				asset_x_balance: FixedU128::from_inner(asset_x_balance),
				asset_y_balance: FixedU128::from_inner(asset_y_balance),
				price_ratio: FixedU128::from_inner(price_ratio),
				lp_token: lp_token,
				lp_token_balance: FixedU128::from_inner(lp_token_balance)
			};

			let account_liquidity_pool_key = (who.clone(), asset_pairs.clone());
			let existing_liquidity_pools = AccountLiquidityPoolStorage::<T>::get(account_liquidity_pool_key)
				.ok_or::<Error<T>>(Error::<T>::AccountLiquidityPoolDoesNotExists.into())
				.and_then(|value| Ok(value))?;

			let account_liquidity_pools = existing_liquidity_pools.to_vec();
			if account_liquidity_pools.len() > 0 {
				AccountLiquidityPoolStorage::<T>::mutate(
					(who, asset_pairs), |query| {
						let mut bounded_vec_account_liquidity_pools: BoundedVec<AccountLiquidityPool<T>, ConstU32<100>> = Default::default();

						if let Some(existing_bounded_vec_account_liquidity_pools) = query {
							let mut last_index = 0u32.into();
							if let Some(account_liquidity_pool) = existing_bounded_vec_account_liquidity_pools.last() {
								last_index = account_liquidity_pool.index;
							}

							new_account_liquidity_pool.index = last_index.ensure_add(1).expect(Error::<T>::AccountLiquidityIndexArithmeticError.into());
							existing_bounded_vec_account_liquidity_pools.try_push(new_account_liquidity_pool.clone()).expect(Error::<T>::AccountLiquidityBoundedVecError.into());

							bounded_vec_account_liquidity_pools = existing_bounded_vec_account_liquidity_pools.clone();
						}

						*query = Some(bounded_vec_account_liquidity_pools)
					}
				);
			} else {
				let mut bounded_vec_account_liquidity_pools: BoundedVec<AccountLiquidityPool<T>, ConstU32<100>> = Default::default();
				bounded_vec_account_liquidity_pools.try_push(new_account_liquidity_pool.clone()).expect(Error::<T>::AccountLiquidityBoundedVecError.into());

				AccountLiquidityPoolStorage::<T>::insert(
					(who, asset_pairs),
					bounded_vec_account_liquidity_pools,
				);
			}

			Self::deposit_event(Event::LiquidityPoolProvidedSuccessfully {
				account_liquidity_pool: new_account_liquidity_pool.clone()
			});

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn redeem_liquidity(
			origin: OriginFor<T>,
			index: AccountLiquidityPoolIndex,
			asset_x: AssetIdOf<T>,
			asset_y: AssetIdOf<T>,
			lp_token: AssetIdOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			let asset_pairs = AssetPairs::<T> { asset_x, asset_y };
			let account_liquidity_pool_key = (who.clone(), asset_pairs.clone());

			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			).expect(Error::<T>::LiquidityPoolError.into());

			let asset_xy_balances = Self::compute_xy_assets(
				who.clone(),
				index,
				lp_token,
				asset_pairs.clone()
			).expect(Error::<T>::RedeemLiquidityPoolBalancesError.into());

			let asset_x_balance = asset_xy_balances.0;
			let asset_y_balance = asset_xy_balances.1;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_x,
				&dex_account_id.clone(),
				&who.clone(),
				asset_x_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_y,
				&dex_account_id.clone(),
				&who.clone(),
				asset_y_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let update_asset_x_balance = existing_liquidity_pool.asset_x_balance.sub(FixedU128::from_inner(asset_x_balance));
			let update_asset_y_balance = existing_liquidity_pool.asset_y_balance.sub(FixedU128::from_inner(asset_y_balance));

			LiquidityPoolStorage::<T>::mutate(
				asset_pairs.clone(), |query| {
					*query = Some(
						LiquidityPool::<T> {
							asset_pairs: asset_pairs.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							lp_token: lp_token,
						}
					);
				}
			);

			AccountLiquidityPoolStorage::<T>::remove(account_liquidity_pool_key);

			Self::deposit_event(Event::LiquidityPoolRedeemedSuccessfully);

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn swap_exact_in_for_out(
			origin: OriginFor<T>,
			asset_exact_in: AssetIdOf<T>,
			asset_exact_in_balance: AssetBalanceOf<T>,
			asset_max_out: AssetIdOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			let asset_pairs = AssetPairs::<T> { asset_x: asset_exact_in, asset_y: asset_max_out };

			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			).expect(Error::<T>::LiquidityPoolError.into());

			let price_ratio = Self::compute_and_get_latest_price(
				asset_pairs.clone()
			).expect(Error::<T>::ComputePriceRatioError.into());

			let asset_max_out_balance = FixedU128::from_inner(price_ratio).mul(FixedU128::from_inner(asset_exact_in_balance));

			Self::check_asset_in_and_out_balances(
				who.clone(),
				asset_exact_in,
				asset_exact_in_balance,
				asset_max_out,
				asset_max_out_balance.into_inner(),
			).expect(Error::<T>::CheckAssetInAndOutBalancesError.into());

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_exact_in,
				&who.clone(),
				&dex_account_id.clone(),
				asset_exact_in_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_max_out,
				&dex_account_id.clone(),
				&who.clone(),
				asset_max_out_balance.into_inner(),
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let update_asset_x_balance = existing_liquidity_pool.asset_x_balance.add(FixedU128::from_inner(asset_exact_in_balance));
			let update_asset_y_balance = existing_liquidity_pool.asset_y_balance.sub(asset_max_out_balance);

			LiquidityPoolStorage::<T>::mutate(
				asset_pairs.clone(), |query| {
					if let Some(liquidity_pool) = query {
						*query = Some(
							LiquidityPool::<T> {
							asset_pairs: asset_pairs.clone(),
							asset_x_balance: update_asset_x_balance,
							asset_y_balance: update_asset_y_balance,
							lp_token: liquidity_pool.lp_token,
						});
					}
				}
			);

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		pub fn swap_in_for_exact_out(
			origin: OriginFor<T>,
			asset_exact_out: AssetIdOf<T>,
			asset_exact_out_balance: AssetBalanceOf<T>,
			asset_min_in: AssetIdOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let dex_account_id = Self::get_dex_account();
			let asset_pairs = AssetPairs::<T> { asset_x: asset_exact_out, asset_y: asset_min_in };

			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			).expect(Error::<T>::LiquidityPoolError.into());

			let price_ratio = Self::compute_and_get_latest_price(
				asset_pairs.clone()
			).expect(Error::<T>::ComputePriceRatioError.into());

			let asset_min_in_balance = FixedU128::from_inner(price_ratio).mul(FixedU128::from_inner(asset_exact_out_balance));

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_min_in,
				&who.clone(),
				&dex_account_id.clone(),
				asset_min_in_balance.into_inner(),
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			<T::Fungibles as fungibles::Mutate<_>>::transfer(
				asset_exact_out,
				&dex_account_id.clone(),
				&who.clone(),
				asset_exact_out_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			let update_asset_x_balance = existing_liquidity_pool.asset_x_balance.add(asset_min_in_balance);
			let update_asset_y_balance = existing_liquidity_pool.asset_y_balance.sub(FixedU128::from_inner(asset_exact_out_balance));

			LiquidityPoolStorage::<T>::mutate(
				asset_pairs.clone(), |query| {
					if let Some(liquidity_pool) = query {
						*query = Some(
							LiquidityPool::<T> {
								asset_pairs: asset_pairs.clone(),
								asset_x_balance: update_asset_x_balance,
								asset_y_balance: update_asset_y_balance,
								lp_token: liquidity_pool.lp_token,
							}
						);
					}
				}
			);

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn get_dex_account() -> T::AccountId {
			DEX_PALLET.into_account_truncating()
		}

		pub fn get_liquidity_pool(
			asset_pairs: AssetPairs<T>,
		) -> Result<LiquidityPool<T>, DispatchError> {
			let existing_liquidity_pool = LiquidityPoolStorage::<T>::get(asset_pairs.clone());
			match existing_liquidity_pool {
				Some(liquidity_pool) => {
					return Ok(liquidity_pool)
				},
				None => {
					let swap_asset_pair = AssetPairs::<T> {
						asset_x: asset_pairs.clone().asset_y,
						asset_y: asset_pairs.clone().asset_x
					};

					let existing_liquidity_pool_swapped_pairs = LiquidityPoolStorage::<T>::get(swap_asset_pair)
						.ok_or::<Error<T>>(Error::<T>::LiquidityPoolDoesNotExists.into())
						.and_then(|value| Ok(value))?;

					return Ok(existing_liquidity_pool_swapped_pairs)
				}
			}
		}

		pub fn get_account_liquidity_pool(
			index: AccountLiquidityPoolIndex,
			account_id: <T as frame_system::Config>::AccountId,
			asset_pairs: AssetPairs<T>
		) -> Result<AccountLiquidityPool<T>, DispatchError> {
			let account_liquidity_pool_key = (account_id.clone(), asset_pairs.clone());
			let existing_bounded_vec_account_liquidity_pools = AccountLiquidityPoolStorage::<T>::get(account_liquidity_pool_key)
				.ok_or::<Error<T>>(Error::<T>::AccountLiquidityPoolDoesNotExists.into())
				.and_then(|value| Ok(value))?;

			let account_liquidity_pools = existing_bounded_vec_account_liquidity_pools.to_vec();
			if account_liquidity_pools.len() > 0 {
				for account_liquidity_pool in account_liquidity_pools {
					if account_liquidity_pool.index == index {
						return Ok(account_liquidity_pool)
					}
				}
			}

			return Err(Error::<T>::AccountLiquidityPoolError.into())
		}

		pub fn compute_and_mint_lp_token(
			asset_pairs: AssetPairs<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y_balance: AssetBalanceOf<T>,
		) -> Result<(AssetIdOf<T>, AssetBalanceOf<T>), DispatchError> {
			let mut lp_token: AssetIdOf<T> = 1u32.into();
			let dex_account_id = Self::get_dex_account();

			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			);

			match existing_liquidity_pool {
				Ok(liquidity_pool) => {
					lp_token = liquidity_pool.lp_token;
				},
				Err(e) => {
					let mut counter = 1u32.into();

					loop {
						lp_token = counter;

						if !<T::Fungibles as fungibles::Inspect<_>>::asset_exists(lp_token) {
							<T::Fungibles as fungibles::Create<_>>::create(
								lp_token,
								dex_account_id.clone(),
								true,
								1u32.into(),
							)?;

							break;
						}

						counter += 1;
					}
				}
			}

			let mul_asset_xy = FixedU128::from_inner(asset_x_balance).mul(FixedU128::from_inner(asset_y_balance));
			if mul_asset_xy.is_zero() {
				return Err(Error::<T>::ComputeAndMintLiquidityPoolTokenArithmeticError.into())
			}

			let lp_token_balance = mul_asset_xy.sqrt().into_inner();

			<T::Fungibles as fungibles::Mutate<_>>::mint_into(
				lp_token,
				&dex_account_id.clone(),
				lp_token_balance,
			)?;

			Ok((lp_token, lp_token_balance))
		}

		pub fn compute_and_get_latest_price(
			asset_pairs: AssetPairs<T>
		) -> Result<u128, DispatchError> {
			let existing_liquidity_pool = Self::get_liquidity_pool(
				asset_pairs.clone()
			).expect(Error::<T>::LiquidityPoolError.into());

			if existing_liquidity_pool.asset_x_balance.is_zero() || existing_liquidity_pool.asset_y_balance.is_zero() {
				return Err(Error::<T>::ComputePriceRatioArithmeticError.into())
			}

			let lp_asset_x_balance = existing_liquidity_pool.asset_x_balance.into_inner();
			let lp_asset_y_balance = existing_liquidity_pool.asset_y_balance.into_inner();

			let price_ratio = FixedU128::from_rational(lp_asset_x_balance, lp_asset_y_balance);

			Ok(price_ratio.into_inner())
		}

		pub fn compute_xy_assets(
			account_id: <T as frame_system::Config>::AccountId,
			index: AccountLiquidityPoolIndex,
			lp_token: AssetIdOf<T>,
			asset_pairs: AssetPairs<T>,
		) -> Result<(AssetBalanceOf<T>, AssetBalanceOf<T>), DispatchError> {
			let existing_account_liquidity = Self::get_account_liquidity_pool(
				index,
				account_id.clone(),
				asset_pairs.clone(),
			).expect(Error::<T>::AccountLiquidityPoolError.into());

			let current_lp_token_balance = existing_account_liquidity.lp_token_balance;

			Self::check_lp_token_balance(
				account_id,
				lp_token,
				current_lp_token_balance.into_inner()
			).expect(Error::<T>::CheckLiquidityPoolTokenBalanceError.into());

			let price_ratio = Self::compute_and_get_latest_price(
				asset_pairs
			).expect(Error::<T>::ComputePriceRatioError.into());

			let get_asset_x_balance = current_lp_token_balance.div(FixedU128::from_inner(price_ratio).sqrt());
			let get_asset_y_balance = current_lp_token_balance.mul(FixedU128::from_inner(price_ratio).sqrt());

			Ok((get_asset_x_balance.into_inner(), get_asset_y_balance.into_inner()))
		}

		pub fn get_asset_balance(
			asset: AssetIdOf<T>,
			account_id: <T as frame_system::Config>::AccountId,
		) -> AssetBalanceOf<T> {
			let balance = <T::Fungibles as fungibles::Inspect<_>>::balance(asset, &account_id);
			balance
		}

		pub fn check_xy_assets_balances(
			account_id: <T as frame_system::Config>::AccountId,
			asset_pairs: AssetPairs<T>,
			asset_x_balance: AssetBalanceOf<T>,
			asset_y_balance: AssetBalanceOf<T>,
		) -> Result<(), DispatchError> {
			let current_asset_x_balance = Self::get_asset_balance(asset_pairs.asset_x, account_id.clone());
			let current_asset_y_balance = Self::get_asset_balance(asset_pairs.asset_y, account_id);

			current_asset_x_balance
				.ensure_sub(asset_x_balance)
				.expect(Error::<T>::AssetXDoesNotHaveEnoughBalance.into());

			current_asset_y_balance
				.ensure_sub(asset_y_balance)
				.expect(Error::<T>::AssetYDoesNotHaveEnoughBalance.into());

			Ok(())
		}

		pub fn check_lp_token_balance(
			account_id: <T as frame_system::Config>::AccountId,
			lp_token: AssetIdOf<T>,
			lp_token_balance: AssetBalanceOf<T>,
		) -> Result<(), DispatchError> {
			let current_lp_token_balance = Self::get_asset_balance(lp_token, account_id);

			current_lp_token_balance
				.ensure_sub(lp_token_balance)
				.expect(Error::<T>::LiquidityPoolTokenDoesNotHaveEnoughBalance.into());

			Ok(())
		}

		pub fn check_asset_in_and_out_balances(
			account_id: <T as frame_system::Config>::AccountId,
			asset_in: AssetIdOf<T>,
			asset_in_balance: AssetBalanceOf<T>,
			asset_out: AssetIdOf<T>,
			asset_out_balance: AssetBalanceOf<T>,
		) -> Result<(), DispatchError> {
			let current_asset_in_balance = Self::get_asset_balance(asset_in, account_id.clone());

			current_asset_in_balance
				.ensure_sub(asset_in_balance)
				.expect(Error::<T>::AssetInDoesNotHaveEnoughBalance.into());

			let dex_account_id = Self::get_dex_account();
			let current_asset_out_balance = Self::get_asset_balance(asset_out, dex_account_id.clone());

			current_asset_out_balance
				.ensure_sub(asset_out_balance)
				.expect(Error::<T>::AssetOutDoesNotHaveEnoughBalance.into());

			Ok(())
		}
	}
}
