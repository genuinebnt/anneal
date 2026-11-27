use solution::*;

fn posting(page: u32, positions: &[u32]) -> Posting {
    Posting {
        page,
        positions: positions.to_vec(),
    }
}

#[test]
fn words() {
    check!(
        "tokenize",
        tokenize("Memory-ordering, ACQUIRE & 3D café!"),
        ["memory", "ordering", "acquire", "3d", "café"].map(String::from).to_vec()
    );
    check!("no words", tokenize(" -- ! "), Vec::<String>::new());
}

#[test]
fn the_example_from_the_statement() {
    let list = [posting(3, &[1, 4]), posting(200, &[0])];
    let bytes = vec![0x02, 0x03, 0x02, 0x01, 0x03, 0xC5, 0x01, 0x01, 0x00];
    check!("encode", encode(&list), bytes.clone());
    check!("decode", decode(&bytes), Ok(list.to_vec()));
}

#[test]
fn an_empty_list() {
    check!("encode", encode(&[]), vec![0]);
    check!("decode [0]", decode(&[0]), Ok(vec![]));
    check!("decode nothing", decode(&[]), Err(CodecError::Truncated));
}

#[test]
fn large_numbers() {
    let list = [posting(u32::MAX, &[0, u32::MAX])];
    let bytes = encode(&list);
    check!("length: 1 + 5 + 1 + 1 + 5", bytes.len(), 13);
    check!("round trip", decode(&bytes), Ok(list.to_vec()));
}

#[test]
fn some_broken_lists() {
    check!("cut off", decode(&[1, 3, 2, 1]), Err(CodecError::Truncated));
    check!("left over", decode(&[0, 0]), Err(CodecError::TrailingBytes));
    check!("page 3 twice", decode(&[2, 3, 1, 0, 0, 1, 0]), Err(CodecError::NotAscending));
    check!("no positions", decode(&[1, 3, 0]), Err(CodecError::NoPositions));
}
