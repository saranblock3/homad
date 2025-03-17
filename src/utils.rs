use rand::Rng;
use std::ops::Range;
use std::os::unix::net::UnixStream;

pub fn split_unix_stream(stream: UnixStream) -> Result<(UnixStream, UnixStream), String> {
    let other = stream
        .try_clone()
        .map_err(|_| "Failed to create split UnixStream")?;
    Ok((stream, other))
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

pub fn fuzz_timeout(timeout_base: u64) -> u64 {
    let timeout_range = (timeout_base - timeout_base / 2)..(timeout_base + timeout_base / 2);
    rand::thread_rng().gen_range::<u64, Range<u64>>(timeout_range)
}
