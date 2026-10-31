#![cfg(test)]

pub fn number() -> u8 {
    "1".parse::<u8>().unwrap()
}
