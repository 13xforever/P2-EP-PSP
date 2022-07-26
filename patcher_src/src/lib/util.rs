use std::io::{prelude::*, SeekFrom};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};

#[macro_export]
macro_rules! align {
    ($v:expr, $a:expr) => {
        ($v + $a - 1) & !($a - 1)
    };
}
pub trait BinaryStruct {
    fn read<R: Read + Seek>(read: &mut R) -> std::io::Result<Box<Self>>;
}

pub fn read_u16bi<R: Read + Seek>(read: &mut R) -> std::io::Result<u16> {
    let le = read.read_u16::<LittleEndian>()?;
    let be = read.read_u16::<BigEndian>()?;
    assert_eq!(le, be);
    Ok(le)
}
pub fn read_u32bi<R: Read + Seek>(read: &mut R) -> std::io::Result<u32> {
    let le = read.read_u32::<LittleEndian>()?;
    let be = read.read_u32::<BigEndian>()?;
    assert_eq!(le, be);
    Ok(le)
}

pub fn write_u16bi<W: Write + Seek>(write: &mut W, value: u16) -> std::io::Result<()> {
    write.write_u16::<LittleEndian>(value)?;
    write.write_u16::<BigEndian>(value)
}
pub fn write_u32bi<W: Write + Seek>(write: &mut W, value: u32) -> std::io::Result<()> {
    write.write_u32::<LittleEndian>(value)?;
    write.write_u32::<BigEndian>(value)
}

pub fn read_cstring<R: Read + Seek>(r: &mut R) -> std::io::Result<String> {
    let mut s = String::new();
    loop {
        let n = r.read_u8()?;
        if n == 0 {
            break;
        } else {
            s.push(n as char)
        }
    }

    Ok(s)
}
pub fn read_cstring_at<R: Read + Seek>(r: &mut R, pos: u64) -> std::io::Result<String> {
    let curr = r.stream_position()?;
    r.seek(SeekFrom::Start(pos))?;
    let str = read_cstring(r)?;
    r.seek(SeekFrom::Start(curr))?;
    Ok(str)
}
pub fn read_cstring_n<R: Read + Seek>(r: &mut R, n: usize) -> std::io::Result<String> {
    let mut s = String::new();
    let mut i = 0;
    loop {
        let c = r.read_u8()?;
        if c == 0 || i == n {
            break;
        } else {
            s.push(c as char)
        }
        i += 1;
    }
    if i != n {
        r.seek(SeekFrom::Current((n - i) as i64))?;
    }

    Ok(s)
}
pub fn read_string_n<R: Read + Seek>(r: &mut R, n: usize) -> std::io::Result<String> {
    let mut vec = vec![0; n];
    r.read_exact(vec.as_mut_slice())?;
    Ok(vec.into_iter().map(|x| x as char).collect())
}
pub fn read_string_trim<R: Read + Seek>(r: &mut R, n: usize) -> std::io::Result<String> {
    let mut vec = vec![0; n];
    r.read_exact(vec.as_mut_slice())?;
    Ok(vec
        .into_iter()
        .map(|x| x as char)
        .collect::<String>()
        .trim_end()
        .to_string())
}
pub fn write_string_pad<W: Write + Seek>(w: &mut W, str: &str, n: usize) -> std::io::Result<()> {

    w.write_all(str.as_bytes())?;
    for _ in 0..n-str.len() {
        w.write_u8(0x20)?; //space
    }
    Ok(())
}

pub fn read_bytes_at<R: Read + Seek>(r: &mut R, pos: u32, size: u32) -> std::io::Result<Vec<u8>> {
    let curr = r.stream_position()?;
    r.seek(SeekFrom::Start(pos as u64))?;
    let mut data = vec![0u8; size as usize];
    r.read_exact(&mut data)?;
    r.seek(SeekFrom::Start(curr))?;
    Ok(data)
}

pub fn write_bytes_at<W: Write + Seek>(w: &mut W, pos: u32, bytes: &[u8]) -> std::io::Result<()> {
    let curr = w.stream_position()?;
    w.seek(SeekFrom::Start(pos as u64))?;
    w.write(bytes)?;
    w.seek(SeekFrom::Start(curr))?;
    Ok(())
}
pub fn write_string<W: Write + Seek>(w: &mut W, s: &str) -> std::io::Result<()> {
    for b in s.bytes() {
        w.write_u8(b)?
    }
    Ok(())
}

pub fn write_string_at<W: Write + Seek>(w: &mut W, pos: u32, s: &str) -> std::io::Result<u32> {
    let curr = w.stream_position()?;
    w.seek(SeekFrom::Start(pos as u64))?;
    for b in s.bytes() {
        w.write_u8(b)?
    }
    let diff = (w.stream_position()? as u32) - pos;
    w.seek(SeekFrom::Start(curr))?;
    Ok(diff as u32)
}
pub fn write_cstring<W: Write + Seek>(w: &mut W, s: &str) -> std::io::Result<()> {
    for b in s.bytes() {
        w.write_u8(b)?
    }
    w.write_u8(0)?;
    Ok(())
}

pub fn write_cstring_at<W: Write + Seek>(w: &mut W, pos: u32, s: &str) -> std::io::Result<u32> {
    // println!("Writing {} at {:#}", s, pos);
    let curr = w.stream_position()?;
    w.seek(SeekFrom::Start(pos as u64))?;
    for b in s.bytes() {
        w.write_u8(b)?
    }
    w.write_u8(0)?;
    let diff = (w.stream_position()? as u32) - pos;
    w.seek(SeekFrom::Start(curr))?;
    Ok(diff)
}
