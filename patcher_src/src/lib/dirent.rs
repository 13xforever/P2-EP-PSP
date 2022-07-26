use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use chrono::{DateTime, Datelike, Local, Timelike};
use std::{
    alloc::System,
    fmt::Debug,
    io::{Read, Seek, Write},
    time::SystemTime,
};

use super::util::{
    read_string_n, read_u16bi, read_u32bi, write_string, write_u16bi, write_u32bi, BinaryStruct,
};
use crate::align;

// use super::util::*;

#[derive(Clone)]
pub struct DirEntTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub timezone: i8,
}

impl DirEntTime {
    fn write<W: Write + Seek>(&self, write: &mut W) -> std::io::Result<()> {
        write.write_u8((self.year - 1900) as u8)?;
        write.write_u8(self.month)?;
        write.write_u8(self.day)?;
        write.write_u8(self.hour)?;
        write.write_u8(self.minute)?;
        write.write_u8(self.second)?;
        write.write_i8(self.timezone)
    }
}

impl Debug for DirEntTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}/{}/{} {}:{}:{}{:+}",
            self.day,
            self.month,
            self.year,
            self.hour,
            self.minute,
            self.second,
            &(self.timezone as f32) / 4.0
        ))
    }
}

impl From<SystemTime> for DirEntTime {
    fn from(t: SystemTime) -> Self {
        let time: DateTime<Local> = DateTime::from(t);

        Self {
            year: time.year_ce().1 as u16,
            month: time.month() as u8,
            day: time.day() as u8,
            hour: time.hour() as u8,
            minute: time.minute() as u8,
            second: time.second() as u8,
            timezone: (time.offset().local_minus_utc() / 60 / 15) as i8,
        }
    }
}

impl BinaryStruct for DirEntTime {
    fn read<R: Read + Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        Ok(Box::new(Self {
            year: read.read_u8()? as u16 + 1900,
            month: read.read_u8()?,
            day: read.read_u8()?,
            hour: read.read_u8()?,
            minute: read.read_u8()?,
            second: read.read_u8()?,
            timezone: read.read_i8()?,
        }))
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DirEnt {
    pub length: u8,
    pub ext_attr: u8,
    pub sector: u32,
    pub size: u32,
    pub time: Box<DirEntTime>,
    pub flags: u8,
    pub unit_size: u8,
    pub gap: u8,
    pub volume: u16,
    pub name: String,
    pub has_xa: bool,
}

impl TryFrom<&std::path::Path> for DirEnt {
    type Error = std::io::Error;

    fn try_from(ent: &std::path::Path) -> Result<Self, Self::Error> {
        let meta = ent.metadata()?;
        let name: String = ent.file_name().unwrap().to_str().unwrap().into();
        let length = align!(33 + name.len(), 2) as u8;
        let ext_attr = 0;
        let sector = 0;
        let size = if meta.is_dir() { 0 } else { meta.len() as u32 };
        //setting it to a constant so the patched ISO can have a consistent checksum...
        // let time = Box::new(DirEntTime::from(SystemTime::now()));
        let time = Box::new(DirEntTime { year: 2012, month: 3, day: 14, hour: 11, minute: 21, second: 00, timezone: 36 });
        let flags = if meta.is_dir() { 0x2 } else { 0 };
        let unit_size = 0;
        let gap = 0;
        let volume = 1;
        let has_xa = false;
        Ok(Self {
            length,
            ext_attr,
            sector,
            size,
            time,
            flags,
            unit_size,
            gap,
            volume,
            name,
            has_xa,
        })
    }
}
impl DirEnt {
    pub fn update_length(&mut self) {
        self.length = align!(33 + self.name.len(), 2) as u8;
        if self.has_xa {
            self.length += 14;
        }
    }
    pub fn set_xa(&mut self, xa: bool) {
        match (self.has_xa, xa) {
            (true, true) => (),
            (true, false) => self.length -= 14,
            (false, true) => self.length += 14,
            (false, false) => (),
        }
        self.has_xa = xa;
    }
    pub fn set_size(&mut self, size: u32) {
        self.size = size;
    }
    pub fn write<W: Write + Seek>(&self, write: &mut W) -> std::io::Result<()> {
        write.write_u8(self.length)?;
        write.write_u8(self.ext_attr)?;
        write_u32bi(write, self.sector)?;
        write_u32bi(write, self.size)?;
        self.time.write(write)?;
        write.write_u8(self.flags)?;
        write.write_u8(self.unit_size)?;
        write.write_u8(self.gap)?;
        write_u16bi(write, self.volume)?;
        write.write_u8(self.name.len() as u8)?;
        write_string(write, &self.name)?;
        if (self.name.len() & 1) == 0 {
            write.write_u8(0)?;
        }
        if self.has_xa {
            write.write_u32::<LittleEndian>(0)?;
            write.write_u8(0x8d)?;
            write_string(write, "UXA")?;
            write.write_u32::<LittleEndian>(0)?; //pad out the rest
            write.write_u16::<LittleEndian>(0)?;
        }
        Ok(())
    }
}

impl BinaryStruct for DirEnt {
    fn read<R: Read + Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        let start = read.stream_position()?;
        let length = read.read_u8()?;
        let ext_attr = read.read_u8()?;
        let sector = read_u32bi(read)?;
        let size = read_u32bi(read)?;
        let time = DirEntTime::read(read)?;
        let flags = read.read_u8()?;
        let unit_size = read.read_u8()?;
        let gap = read.read_u8()?;
        let volume = read_u16bi(read)?;
        let name_len = read.read_u8()?;
        let name = read_string_n(read, name_len as usize)?;

        let end = read.stream_position()?;

        read.seek(std::io::SeekFrom::Start(start + length as u64))?;
        Ok(Box::new(Self {
            length,
            ext_attr,
            sector,
            size,
            time,
            flags,
            unit_size,
            gap,
            volume,
            name,
            has_xa: (length as u64) != end - start,
        }))
    }
}
