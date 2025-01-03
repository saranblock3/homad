use sscanf::scanf;

pub fn ipv4_string_to_bytes(ipv4: &str) -> Result<[u8; 4], sscanf::Error> {
    let (i, j, k, l) = scanf!(ipv4, "{u8}.{u8}.{u8}.{u8}")?;
    Ok([i, j, k, l])
}

pub fn ipv4_bytes_to_string(ipv4: &[u8; 4]) -> String {
    let [i, j, k, l] = ipv4;
    format!("{}.{}.{}.{}", i, j, k, l)
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn string_to_bytes() {
        assert_eq!([1, 2, 3, 4], ipv4_string_to_bytes("1.2.3.4").unwrap());
    }

    #[test]
    fn bytes_to_string() {
        assert_eq!("1.2.3.4", ipv4_bytes_to_string(&[1, 2, 3, 4]));
    }

    #[test]
    fn bytes_to_string_and_back() {
        assert_eq!(
            [1, 2, 3, 4],
            ipv4_string_to_bytes(&ipv4_bytes_to_string(&[1, 2, 3, 4])).unwrap()
        );
    }

    #[test]
    fn string_to_bytes_and_back() {
        assert_eq!(
            "192.168.1.68",
            ipv4_bytes_to_string(&ipv4_string_to_bytes("192.168.1.68").unwrap())
        );
    }
}
