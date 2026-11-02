use solution::*;

#[test]
fn send_recv_in_order() {
    check!(r#"send 1, 2, 3"#, { let (tx, rx) = channel(3); tx.send(1).unwrap(); tx.send(2).unwrap(); tx.send(3).unwrap(); (rx.recv(), rx.recv(), rx.recv()) }, (Ok(1), Ok(2), Ok(3)));
}

#[test]
fn try_recv_empty() {
    check!(r#"nothing sent"#, { let (_tx, rx) = channel::<u8>(2); rx.try_recv() }, Err(TryRecvError::Empty));
}

#[test]
fn try_send_full() {
    check!(r#"capacity 1, second try_send"#, { let (tx, _rx) = channel(1); tx.try_send(1).unwrap(); tx.try_send(2) }, Err(TrySendError::Full(2)));
}

#[test]
fn recv_drains_then_disconnects() {
    check!(r#"send 1, 2, drop sender"#, { let (tx, rx) = channel(4); tx.send(1).unwrap(); tx.send(2).unwrap(); drop(tx); (rx.recv(), rx.recv(), rx.recv(), rx.try_recv()) }, (Ok(1), Ok(2), Err(RecvError), Err(TryRecvError::Disconnected)));
}

#[test]
fn send_after_receiver_gone() {
    check!(r#"drop receiver, then send"#, { let (tx, rx) = channel(2); drop(rx); (tx.send(5), tx.try_send(6)) }, (Err(SendError(5)), Err(TrySendError::Disconnected(6))));
}
