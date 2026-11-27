use solution::*;

#[test]
fn numbers_go_out_in_order_after_the_reserved_ones() {
    let mut lender = Lender::new(10, 3);
    check!("free", lender.free(), 7);
    check!("two takes", (lender.take(), lender.take()), (Some(3), Some(4)));
}

#[test]
fn a_returned_number_waits_its_turn() {
    let mut lender = Lender::new(6, 0);
    for _ in 0..3 {
        lender.take();
    }
    lender.give_back(0).unwrap();
    let next: Vec<Option<u32>> = (0..5).map(|_| lender.take()).collect();
    check!("3, 4, 5 first, then 0, then none", next, vec![Some(3), Some(4), Some(5), Some(0), None]);
}

#[test]
fn the_map_has_one_bit_per_number() {
    let mut lender = Lender::new(12, 1);
    for _ in 0..8 {
        lender.take();
    }
    // numbers 0 to 8: all of byte 0 and bit 0 of byte 1
    check!("bytes", lender.bytes().to_vec(), vec![0xFF, 0x01]);
}

#[test]
fn only_what_was_lent_can_come_back() {
    let mut lender = Lender::new(8, 2);
    lender.take();
    check!("never lent", lender.give_back(5), Err(GiveBackError::NotLent));
    check!("reserved", lender.give_back(1), Err(GiveBackError::Reserved));
    check!("out of range", lender.give_back(8), Err(GiveBackError::OutOfRange));
    check!("once", lender.give_back(2), Ok(()));
    check!("twice", lender.give_back(2), Err(GiveBackError::NotLent));
    check!("free", lender.free(), 6);
}

#[test]
fn a_saved_map_carries_on() {
    let mut lender = Lender::from_bytes(&[0b0000_0101], 4, 0).unwrap();
    check!("free", lender.free(), 2);
    check!("takes", (lender.take(), lender.take(), lender.take()), (Some(1), Some(3), None));
}
