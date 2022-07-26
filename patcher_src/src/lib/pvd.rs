use byteorder::BigEndian;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Local;
use chrono::Timelike;

use super::dirent::*;
use super::util::*;
use std::io::prelude::*;
use std::io::Read;
use std::time::SystemTime;

pub struct PVDTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub centisecond: u8,
    pub timezone: i8,
}

impl std::fmt::Debug for PVDTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}/{}/{} {}:{}:{}.{}{:+}",
            &self.day,
            &self.month,
            &self.year,
            &self.hour,
            &self.minute,
            &self.second,
            &self.centisecond,
            &(self.timezone as f32) / 4.0
        ))
    }
}

impl From<SystemTime> for PVDTime {
    fn from(t: SystemTime) -> Self {
        let time: DateTime<Local> = DateTime::from(t);

        Self {
            year: time.year_ce().1 as u16,
            month: time.month() as u8,
            day: time.day() as u8,
            hour: time.hour() as u8,
            minute: time.minute() as u8,
            second: time.second() as u8,
            centisecond: (time.timestamp_subsec_millis().min(999)/10) as u8,
            timezone: (time.offset().local_minus_utc() / 60 / 15) as i8,
        }
    }
}


impl PVDTime {
    fn write<W: Write + Seek>(&self, write: &mut W) -> std::io::Result<()> {
        write_string(write, &format!("{:04}", self.year))?;
        write_string(write, &format!("{:02}", self.month))?;
        write_string(write, &format!("{:02}", self.day))?;
        write_string(write, &format!("{:02}", self.hour))?;
        write_string(write, &format!("{:02}", self.minute))?;
        write_string(write, &format!("{:02}", self.second))?;
        write_string(write, &format!("{:02}", self.centisecond))?;
        write.write_i8(self.timezone)?;
        Ok(())
    }
}
impl BinaryStruct for PVDTime {
    fn read<R: Read + Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        Ok(Box::new(Self {
            year: read_string_n(read, 4)?.parse().unwrap(),
            month: read_string_n(read, 2)?.parse().unwrap(),
            day: read_string_n(read, 2)?.parse().unwrap(),
            hour: read_string_n(read, 2)?.parse().unwrap(),
            minute: read_string_n(read, 2)?.parse().unwrap(),
            second: read_string_n(read, 2)?.parse().unwrap(),
            centisecond: read_string_n(read, 2)?.parse().unwrap(),
            timezone: read.read_i8()?,
        }))
    }
}

#[derive(Debug)]
pub struct PVD {
    pub pvd_type: u8,
    pub id: String,
    pub version: u8,
    pub system_id: String,
    pub volume_id: String,
    pub volume_space_size: u32,
    pub volume_set_size: u16,
    pub volume_seq_num: u16,
    pub block_size: u16,
    pub path_table_size: u32,
    pub l_sector: u32,
    pub l_sector_opt: u32,
    pub m_sector: u32,
    pub m_sector_opt: u32,
    pub root_ent: Box<DirEnt>,
    pub set_id: String,
    pub pub_id: String,
    pub prep_id: String,
    pub app_id: String,
    pub copyright_file: String,
    pub abstract_file: String,
    pub biblio_file: String,
    pub created: Box<PVDTime>,
    pub modified: Box<PVDTime>,
    pub expired: Box<PVDTime>,
    pub effective: Box<PVDTime>,
    pub file_version: u8,
}

impl PVD {
    pub fn write<W: Write + Seek>(&self, write: &mut W) -> std::io::Result<()> {
        write.write_u8(self.pvd_type)?;
        write_string_pad(write, &self.id, 5)?;
        write.write_u8(self.version)?;
        write.seek(std::io::SeekFrom::Current(1))?;

        write_string_pad(write, &self.system_id, 32)?;
        write_string_pad(write, &self.volume_id, 32)?;
        write.seek(std::io::SeekFrom::Current(8))?;
        write_u32bi(write, self.volume_space_size)?;
        write.seek(std::io::SeekFrom::Current(32))?;

        write_u16bi(write, self.volume_set_size)?;
        write_u16bi(write, self.volume_seq_num)?;
        write_u16bi(write, self.block_size)?;
        write_u32bi(write, self.path_table_size)?;
        write.write_u32::<LittleEndian>(self.l_sector)?;
        write.write_u32::<LittleEndian>(self.l_sector_opt)?;
        write.write_u32::<BigEndian>(self.m_sector)?;
        write.write_u32::<BigEndian>(self.m_sector_opt)?;
        self.root_ent.write(write)?;
        write_string_pad(write, &self.set_id, 128)?;
        write_string_pad(write, &self.pub_id, 128)?;
        write_string_pad(write, &self.prep_id, 128)?;
        write_string_pad(write, &self.app_id, 128)?;
        write_string_pad(write, &self.copyright_file, 37)?;
        write_string_pad(write, &self.abstract_file, 37)?;
        write_string_pad(write, &self.biblio_file, 37)?;
        self.created.write(write)?;
        self.modified.write(write)?;
        self.expired.write(write)?;
        self.effective.write(write)?;
        write.write_u8(self.file_version)?;
        Ok(())
    }
}

impl BinaryStruct for PVD {
    fn read<R: Read + Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        let pvd_type = read.read_u8()?;
        let id = read_string_trim(read, 5)?;
        let version = read.read_u8()?;
        read.seek(std::io::SeekFrom::Current(1))?;
        let system_id = read_string_trim(read, 32)?.trim().to_string();
        let volume_id = read_string_trim(read, 32)?.trim().to_string();
        read.seek(std::io::SeekFrom::Current(8))?;
        let volume_space_size = read_u32bi(read)?;
        read.seek(std::io::SeekFrom::Current(32))?;
        let volume_set_size = read_u16bi(read)?;
        let volume_seq_num = read_u16bi(read)?;
        let block_size = read_u16bi(read)?;
        let path_table_size = read_u32bi(read)?;
        let l_sector = read.read_u32::<LittleEndian>()?;
        let l_sector_opt = read.read_u32::<LittleEndian>()?;
        let m_sector = read.read_u32::<BigEndian>()?;
        let m_sector_opt = read.read_u32::<BigEndian>()?;
        let root_ent = DirEnt::read(read)?;
        let set_id = read_string_trim(read, 128)?.trim().to_string();
        let pub_id = read_string_trim(read, 128)?.trim().to_string();
        let prep_id = read_string_trim(read, 128)?.trim().to_string();
        let app_id = read_string_trim(read, 128)?.trim().to_string();
        let copyright_file = read_string_trim(read, 37)?;
        let abstract_file = read_string_trim(read, 37)?;
        let biblio_file = read_string_trim(read, 37)?;
        let created = PVDTime::read(read)?;
        let modified = PVDTime::read(read)?;
        let expired = PVDTime::read(read)?;
        let effective = PVDTime::read(read)?;
        let file_version = read.read_u8()?;

        Ok(Box::new(Self {
            pvd_type,
            id,
            version,
            system_id,
            volume_id,
            volume_space_size,
            volume_set_size,
            volume_seq_num,
            block_size,
            path_table_size,
            l_sector,
            l_sector_opt,
            m_sector,
            m_sector_opt,
            root_ent,
            set_id,
            pub_id,
            prep_id,
            app_id,
            copyright_file,
            abstract_file,
            biblio_file,
            created,
            modified,
            expired,
            effective,
            file_version,
        }))
    }
}
