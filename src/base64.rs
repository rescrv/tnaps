const BASE64: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l',
    'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', ' ', ' ',
];

fn encode_one(input: [u8; 3]) -> [char; 4] {
    let mut index = [0u8; 4];
    index[0] = input[0] >> 2;
    index[1] = ((input[0] & 0x03) << 4) + (input[1] >> 4);
    index[2] = ((input[1] & 0x0f) << 2) + (input[2] >> 6);
    index[3] = input[2] & 0x3f;
    let mut output = [' '; 4];
    output[0] = BASE64[index[0] as usize];
    output[1] = BASE64[index[1] as usize];
    output[2] = BASE64[index[2] as usize];
    output[3] = BASE64[index[3] as usize];
    output
}

pub fn encode(mut bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity((bytes.len() * 8 + 5) / 6);
    while bytes.len() >= 3 {
        let input = [bytes[0], bytes[1], bytes[2]];
        let output = encode_one(input);
        bytes = &bytes[3..];
        for c in output.into_iter() {
            encoded.push(c);
        }
    }
    let (input, sz) = match bytes.len() {
        2 => ([bytes[0], bytes[1], 0], 3),
        1 => ([bytes[0], 0, 0], 2),
        0 => {
            return encoded;
        }
        _ => {
            unreachable!();
        }
    };
    let output = encode_one(input);
    for c in output.into_iter().take(sz) {
        encoded.push(c);
    }
    encoded
}
