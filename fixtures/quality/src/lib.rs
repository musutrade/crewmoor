// Isolated gate acceptance fixture; not application code.
pub fn choose(value: bool) -> u8 {
    if value { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    fn helper() -> u8 {
        super::test_support::number()
    }
    #[test]
    fn both_paths() {
        assert_eq!(super::choose(true), helper());
        assert_eq!(super::choose(false), 0);
        assert_eq!("2".parse::<u8>().unwrap(), 2);
    }
}

#[cfg(test)]
mod test_support;

#[path = "../external.rs"]
pub mod external;
include!("../included.rs");
