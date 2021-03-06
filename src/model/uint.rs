use std::io::{self, Read, Write};

use byteorder::{ReadBytesExt as _, WriteBytesExt as _};

pub fn read_uint<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut n = 0;
    for shift in (0..).step_by(7) {
        let byte = reader.read_u8()?;
        n |= u64::from(byte & 127)
            .checked_shl(shift)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        if byte & 128 == 0 {
            break;
        }
    }
    Ok(n)
}

pub fn write_uint<W: Write>(writer: &mut W, mut n: u64) -> io::Result<()> {
    while n > 127 {
        writer.write_u8((n as u8 & 127) | 128)?;
        n >>= 7;
    }
    writer.write_u8(n as u8)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use quickcheck::quickcheck;

    use super::*;

    quickcheck! {
        fn test_uint_roundtrip(n: u64) -> bool {
            let mut writer = Cursor::new(Vec::new());
            write_uint(&mut writer, n).unwrap();

            let mut reader = Cursor::new(writer.into_inner());
            read_uint(&mut reader).unwrap() == n
        }
    }
}
