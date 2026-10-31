// Isolated gate acceptance fixture; not application code.
pub fn choose(value: bool) -> u8 {
    if value { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    fn helper() -> u8 {
        "1".parse::<u8>().unwrap()
    }
    #[test]
    fn both_paths() {
        assert_eq!(super::choose(true), helper());
        assert_eq!(super::choose(false), 0);
        assert_eq!("2".parse::<u8>().unwrap(), 2);
    }
}
