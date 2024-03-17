use crate::{mock::*, Error, Event, mock};
use frame_support::{assert_noop, assert_ok};
use frame_support::{
	sp_runtime::{
		traits::{Convert, EnsureAdd, EnsureDiv, EnsureMul, EnsureSub, IntegerSquareRoot, Zero},
		ArithmeticError, FixedU128, Perbill,
	},
	traits::{fungible, fungibles},
};

#[test]
fn create_liquidity_pool_works() {
	new_test_ext().execute_with(|| {
		let bob = RuntimeOrigin::signed(2);
		let first_balance_a: u128 = 15_000_000_000_000_000_000_000_000;
		let first_balance_b: u128 = 20_000_000_000_000_000_000_000_000;

		assert_ok!(Dex::create_liquidity_pool(bob.clone(), 1, first_balance_a, 2, first_balance_b));

		println!("");

		println!("Dex Account: {:?}", Dex::get_dex_account());
		println!("");

		println!("===============Create Liquidity===================");
		println!("Bob Liquidity Provide for Asset A: {:?}", FixedU128::from_inner(first_balance_a));
		println!("Bob Liquidity Provide for Asset B: {:?}", FixedU128::from_inner(first_balance_b));
		println!("Bob LP Token Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(3, 2)));
		println!("Bob Asset A Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(1, 2)));
		println!("Bob Asset B Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(2, 2)));
		println!("===============Create Liquidity===================");
		println!("");

		let second_balance_a: u128 = 3_700_000_000_000_000_000_000_000;
		let second_balance_b: u128 = 2_100_000_000_000_000_000_000_000;

		assert_ok!(Dex::provide_liquidity(bob, 1, second_balance_a, 2, second_balance_b));

		println!("===============Bob Provided another Liquidity===================");
		println!("Bob Liquidity Provide for Asset A: {:?}", FixedU128::from_inner(second_balance_a));
		println!("Bob Liquidity Provide for Asset B: {:?}", FixedU128::from_inner(second_balance_b));
		println!("Bob LP Token Balance (Plus the previous LP Token - But in the ledger storage it is separated): {:?}", FixedU128::from_inner(Dex::get_asset_balance(3, 2)));
		println!("Bob Asset A Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(1, 2)));
		println!("Bob Asset B Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(2, 2)));
		println!("===============Bob Provided another Liquidity===================");
		println!("");

		println!("=============================================================");
		println!("Dex Asset A Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(1, Dex::get_dex_account())));
		println!("Dex Asset B Balance: {:?}", FixedU128::from_inner(Dex::get_asset_balance(2, Dex::get_dex_account())));
		println!("=============================================================");
	});
}
