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
		
	});
}
