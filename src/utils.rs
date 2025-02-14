use std::{
    collections::VecDeque,
    ops::{Add, Div, Rem},
    usize,
};

use num::{Float, Integer, Num};
use sscanf::scanf;

#[allow(unused)]
pub fn ipv4_string_to_bytes(ipv4: &str) -> Result<[u8; 4], sscanf::Error> {
    let (i, j, k, l) = scanf!(ipv4, "{u8}.{u8}.{u8}.{u8}")?;
    Ok([i, j, k, l])
}

pub fn ipv4_bytes_to_string(ipv4: &[u8; 4]) -> String {
    let [i, j, k, l] = ipv4;
    format!("{}.{}.{}.{}", i, j, k, l)
}

pub fn median(v: Vec<u64>) -> u64 {
    if v.len() == 0 {
        return 0;
    }
    if v.len() % 2 == 0 {
        let i = v[v.len() / 2 - 1];
        let j = v[v.len() / 2];
        return (i + j) / 2;
    }
    return v[v.len() / 2];
}

pub fn lower_quartile(v: &Vec<u64>) -> u64 {
    if v.len() == 0 {
        return 0;
    }
    let first_half = v[0..v.len() / 2].to_vec();
    median(first_half)
}

pub fn upper_quartile(v: &Vec<u64>) -> u64 {
    if v.len() == 0 {
        return 0;
    }
    if v.len() % 2 == 0 {
        let second_half = v[v.len() / 2..v.len()].to_vec();
        return median(second_half);
    }
    let second_half = v[v.len() / 2 + 1..v.len()].to_vec();
    median(second_half)
}

pub fn partitions(v: &Vec<u64>, num_partitions: usize) -> Vec<u64> {
    let mut partitions: Vec<u64> = Vec::new();
    let partition_len = v.len() / num_partitions;
    for partition in (partition_len - 1..v.len() - 1).step_by(partition_len) {
        partitions.push(v[partition])
    }
    partitions
}

pub fn quantile(v: &Vec<u64>, q: f32) -> u64 {
    let pos = (v.len() - 1) as f32 * q;
    let quotient = pos.floor();
    let remainder = pos - (quotient);

    if remainder > 0. {
        let i = quotient as usize;
        let j = quotient as usize + 1;
        let first = v[i] as f32 * (1. - remainder);
        let second = v[j] as f32 * (remainder);
        return first as u64 + second as u64;
    }

    let i = quotient as usize;
    return v[i];
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

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
