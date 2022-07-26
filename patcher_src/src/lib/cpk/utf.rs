use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::lib::util::{
    read_bytes_at, read_cstring_at, read_string_n, write_cstring_at, write_string, BinaryStruct,
};

use crate::align;
use crate::lib::util::*;

use std::{
    collections::HashMap,
    io::{Seek, SeekFrom, Write},
};

#[derive(Debug, Clone)]
pub enum UTFValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    STRING(String),
    BYTES(Box<UTF>),
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum UTFDataType {
    U8 = 0,
    U8_2 = 1,
    U16 = 2,
    U16_2 = 3,
    U32 = 4,
    U32_2 = 5,
    U64 = 6,
    U64_2 = 7,
    // FLOAT = 8,
    STRING = 0xa,
    BYTEARRAY = 0xb,
}
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum UTFStorage {
    NONE = 0,
    ZERO = 1,
    CONSTANT = 3,
    PER_ROW = 5,
}
#[derive(Debug, Clone)]
pub struct UTFColumn {
    pub name: String,
    pub dtype: UTFDataType,
    pub storage: UTFStorage,
    pub value: Option<UTFValue>,
}
#[derive(Debug, Clone)]
pub struct UTF {
    pub name: String,
    pub data_off: u32,
    pub cols: Vec<UTFColumn>,
    pub col_lookup: HashMap<String, usize>,
    pub rows: Vec<HashMap<String, Option<UTFValue>>>,
    pub col_len: u32,
    pub row_len: u16,
    pub str_len: u32,
}

impl From<u8> for UTFDataType {
    fn from(v: u8) -> Self {
        match v & 0xf {
            0 => UTFDataType::U8,
            1 => UTFDataType::U8_2,
            2 => UTFDataType::U16,
            3 => UTFDataType::U16_2,
            4 => UTFDataType::U32,
            5 => UTFDataType::U32_2,
            6 => UTFDataType::U64,
            7 => UTFDataType::U64_2,
            10 => UTFDataType::STRING,
            11 => UTFDataType::BYTEARRAY,
            _ => unreachable!(),
        }
    }
}
impl From<u8> for UTFStorage {
    fn from(v: u8) -> Self {
        match v >> 4 {
            0 => UTFStorage::NONE,
            1 => UTFStorage::ZERO,
            3 => UTFStorage::CONSTANT,
            5 => UTFStorage::PER_ROW,
            _ => unreachable!(),
        }
    }
}
impl Into<u8> for UTFStorage {
    fn into(self) -> u8 {
        self as u8
    }
}
impl Into<u8> for UTFDataType {
    fn into(self) -> u8 {
        self as u8
    }
}
impl UTFValue {
    fn read<R: std::io::Read + std::io::Seek>(
        read: &mut R,
        dtype: UTFDataType,
        str: u32,
        data: u32,
    ) -> std::io::Result<Self> {
        Ok(match dtype {
            UTFDataType::U8 => Self::U8(read.read_u8()?),
            UTFDataType::U8_2 => Self::U8(read.read_u8()?),
            UTFDataType::U16 => Self::U16(read.read_u16::<BigEndian>()?),
            UTFDataType::U16_2 => Self::U16(read.read_u16::<BigEndian>()?),
            UTFDataType::U32 => Self::U32(read.read_u32::<BigEndian>()?),
            UTFDataType::U32_2 => Self::U32(read.read_u32::<BigEndian>()?),
            UTFDataType::U64 => Self::U64(read.read_u64::<BigEndian>()?),
            UTFDataType::U64_2 => Self::U64(read.read_u64::<BigEndian>()?),
            UTFDataType::STRING => {
                let off = read.read_u32::<BigEndian>()?;
                Self::STRING(read_cstring_at(read, (str + off) as u64)?)
            }
            UTFDataType::BYTEARRAY => {
                let off = read.read_u32::<BigEndian>()?;
                let _size = read.read_u32::<BigEndian>()?;
                let here = read.stream_position()?;
                read.seek(SeekFrom::Start((data + off).into()))?;
                let utf = UTF::read(read)?;
                read.seek(SeekFrom::Start(here))?;
                Self::BYTES(utf)
            }
        })
    }
}
impl Into<u32> for &UTFValue {
    fn into(self) -> u32 {
        match self {
            UTFValue::U8(v) => *v as u32,
            UTFValue::U16(v) => *v as u32,
            UTFValue::U32(v) => *v as u32,
            UTFValue::U64(v) => *v as u32,
            UTFValue::STRING(_) => unimplemented!(),
            UTFValue::BYTES(_) => unimplemented!(),
        }
    }
}
impl Into<String> for &UTFValue {
    fn into(self) -> String {
        match self {
            UTFValue::U8(_) => unimplemented!(),
            UTFValue::U16(_) => unimplemented!(),
            UTFValue::U32(_) => unimplemented!(),
            UTFValue::U64(_) => unimplemented!(),
            UTFValue::STRING(v) => v.clone(),
            UTFValue::BYTES(_) => unimplemented!(),
        }
    }
}
impl UTFColumn {
    fn read<R: std::io::Read + std::io::Seek>(
        read: &mut R,
        str: u32,
        data: u32,
    ) -> std::io::Result<Self> {
        let mut flags = read.read_u8()?;
        if flags == 0 {
            read.seek(std::io::SeekFrom::Current(3))?;
            flags = read.read_u8()?;
        }
        let dtype = flags.into();
        let storage = flags.into();
        let name_off = read.read_u32::<BigEndian>()?;
        let name = read_cstring_at(read, (str + name_off) as u64)?;
        let value = if let UTFStorage::CONSTANT = storage {
            Some(UTFValue::read(read, dtype, str, data)?)
        } else {
            None
        };

        Ok(Self {
            name,
            dtype,
            storage,
            value,
        })
    }
}

impl BinaryStruct for UTF {
    fn read<R: std::io::Read + std::io::Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        let base = (read.stream_position()? + 8) as u32;
        let utfname = read_string_n(read, 4)?;
        if utfname != "@UTF" {
            panic!("Not utf");
        }
        let table_size = read.read_u32::<BigEndian>()?;
        let rows_offset = read.read_u32::<BigEndian>()? + base;
        let strings_offset = read.read_u32::<BigEndian>()? + base;
        let data_offset = read.read_u32::<BigEndian>()? + base;

        let table_name = read.read_u32::<BigEndian>()?;
        let num_col = read.read_u16::<BigEndian>()?;
        let row_len = read.read_u16::<BigEndian>()?;
        let num_rows = read.read_u32::<BigEndian>()?;
        let name = read_cstring_at(read, (strings_offset + table_name).into())?;

        let mut col_lookup = HashMap::new();

        // let mut cols = Vec::new();
        // for _ in 0..num_col {
        //     cols.push(UTFColumn::read(read, strings_offset, data_offset)?);
        // }
        let cols = (0..num_col)
            .map(|_| UTFColumn::read(read, strings_offset, data_offset))
            .collect::<std::io::Result<Vec<UTFColumn>>>()?;
        cols.iter().enumerate().for_each(|(i, x)| {
            col_lookup.insert(x.name.clone(), i);
        });
        let mut rows = Vec::new();
        for i in 0..num_rows {
            read.seek(SeekFrom::Start((rows_offset + (row_len as u32) * i) as u64))?;
            let row = cols
                .iter()
                .map(|col| {
                    Ok((
                        col.name.clone(),
                        match col.storage {
                            UTFStorage::NONE => None,
                            UTFStorage::ZERO => None,
                            UTFStorage::CONSTANT => None,
                            UTFStorage::PER_ROW => Some(UTFValue::read(
                                read,
                                col.dtype,
                                strings_offset,
                                data_offset,
                            )?),
                        },
                    ))
                })
                .collect::<std::io::Result<HashMap<String, Option<UTFValue>>>>()?;

            rows.push(row);
        }
        read.seek(SeekFrom::Start((base + table_size) as u64))?;

        Ok(Box::new(Self {
            name,
            data_off: data_offset,
            cols,
            col_lookup,
            rows,
            str_len: 0,
            row_len,
            col_len: 0,
        }))
    }
}

impl UTFValue {
    pub fn write<W: Write + Seek>(
        &self,
        write: &mut W,
        str_off: u32,
        str_ptr: &mut u32,
    ) -> std::io::Result<()> {
        match self {
            UTFValue::U8(v) => write.write_u8(*v),
            UTFValue::U16(v) => write.write_u16::<BigEndian>(*v),
            UTFValue::U32(v) => write.write_u32::<BigEndian>(*v),
            UTFValue::U64(v) => write.write_u64::<BigEndian>(*v),
            UTFValue::STRING(v) => {
                let off = *str_ptr;
                *str_ptr += write_cstring_at(write, str_off + *str_ptr, v)?;
                write.write_u32::<BigEndian>(off)
            }

            UTFValue::BYTES(_) => todo!(),
        }
    }
}

impl UTF {
    pub fn add_col(
        &mut self,
        name: String,
        dtype: UTFDataType,
        storage: UTFStorage,
        value: Option<UTFValue>,
    ) {
        let mut col_val = match storage {
            UTFStorage::NONE => None,
            UTFStorage::ZERO => None,
            UTFStorage::CONSTANT => value,
            UTFStorage::PER_ROW => {
                self.rows.iter_mut().for_each(|x| {
                    x.insert(name.clone(), value.clone());
                });
                None
            }
        };
        self.col_lookup.insert(name.clone(), self.cols.len());
        self.cols.push(UTFColumn {
            name,
            dtype,
            storage,
            value: col_val,
        })
    }
    pub fn get_col_mut(&mut self, name: &str) -> &mut UTFColumn {
        self.cols.get_mut(self.col_lookup[name]).unwrap()
    }
    pub fn remove_column(&mut self, name: &str) {
        // dbg!(name);
        let idx = self.col_lookup[name];
        self.cols.remove(idx);
        self.col_lookup.remove(name);
        self.col_lookup.values_mut().for_each(|x| {
            if *x > idx {
                *x -= 1;
            }
        })
    }
    pub fn calculate_size(&mut self) -> usize {
        let mut row_len = 0;
        let mut col_len = 0;
        let mut str_len = 7 + self.name.len() + 1;
        for col in self.cols.iter() {
            col_len += 5;
            str_len += col.name.len() + 1;
            match col.storage {
                UTFStorage::NONE => (),
                UTFStorage::ZERO => (),
                UTFStorage::CONSTANT => {
                    col_len += match col.dtype {
                        UTFDataType::U8 => 1,
                        UTFDataType::U8_2 => 1,
                        UTFDataType::U16 => 2,
                        UTFDataType::U16_2 => 2,
                        UTFDataType::U32 => 4,
                        UTFDataType::U32_2 => 4,
                        UTFDataType::U64 => 8,
                        UTFDataType::U64_2 => 8,
                        UTFDataType::STRING => {
                            if let Some(UTFValue::STRING(v)) = &col.value {
                                str_len += v.len() + 1;
                            } else {
                                panic!("Mismatched data")
                            }
                            4
                        }
                        UTFDataType::BYTEARRAY => {
                            unimplemented!()
                        }
                    }
                }
                UTFStorage::PER_ROW => {
                    row_len += match col.dtype {
                        UTFDataType::U8 => 1,
                        UTFDataType::U8_2 => 1,
                        UTFDataType::U16 => 2,
                        UTFDataType::U16_2 => 2,
                        UTFDataType::U32 => 4,
                        UTFDataType::U32_2 => 4,
                        UTFDataType::U64 => 8,
                        UTFDataType::U64_2 => 8,
                        UTFDataType::STRING => {
                            str_len += self.rows.iter().fold(0, |p, c| {
                                p + if let Some(UTFValue::STRING(v)) = &c[&col.name] {
                                    v.len() + 1
                                } else {
                                    panic!("Mismatched data")
                                }
                            });
                            4
                        }
                        UTFDataType::BYTEARRAY => {
                            unimplemented!()
                        }
                    }
                }
            }
        }
        self.str_len = str_len as u32;
        // assert_eq!(self.row_len, row_len);
        self.row_len = row_len;
        self.col_len = col_len;

        //header values, @UTF, <NULL>\0 + own name
        align!(
            str_len + (row_len as usize) * self.rows.len() + (col_len as usize) + 6 * 4 + 2 * 2 + 4,
            4
        )
    }

    pub fn write<W: Write + Seek>(&self, write: &mut W) -> std::io::Result<()> {
        let start = write.stream_position()?;
        write_string(write, "@UTF")?;
        let size = align!(
            self.col_len
                + (self.row_len as u32) * (self.rows.len() as u32)
                + (self.str_len as u32)
                + 6 * 4
                + 2 * 2
                + 4
                - 8,
            4
        );

        let row_off = 5 * 4 + 2 * 2 + self.col_len;
        let str_off = row_off + (self.row_len as u32) * (self.rows.len() as u32);
        let mut str_ptr = 0;

        macro_rules! add_str {
            ($s:expr) => {
                write.write_u32::<BigEndian>(str_ptr)?;
                str_ptr += write_cstring_at(write, (start as u32) + 8 + str_ptr + str_off, &$s)?;
            };
        }

        write.write_u32::<BigEndian>(size)?;
        write.write_u32::<BigEndian>(row_off)?;
        write.write_u32::<BigEndian>(str_off)?;
        write.write_u32::<BigEndian>(size)?; //data offset
        str_ptr += write_cstring_at(write, (start as u32) + 8 + str_ptr + str_off, "<NULL>")?;
        add_str!(&self.name);
        write.write_u16::<BigEndian>(self.cols.len() as u16)?;
        write.write_u16::<BigEndian>(self.row_len)?;
        write.write_u32::<BigEndian>(self.rows.len() as u32)?;

        for col in self.cols.iter() {
            let flags = (col.dtype as u8) | ((col.storage as u8) << 4);
            write.write_u8(flags)?;
            add_str!(&col.name);
            match col.storage {
                UTFStorage::CONSTANT => {
                    col.value.as_ref().unwrap().write(
                        write,
                        (start as u32) + 8 + str_off,
                        &mut str_ptr,
                    )?;
                }
                _ => (),
            }
        }
        let row_cols = self
            .cols
            .iter()
            .filter(|x| match x.storage {
                UTFStorage::PER_ROW => true,
                _ => false,
            })
            .collect::<Vec<_>>();
        for row in self.rows.iter() {
            for col in row_cols.iter() {
                row[&col.name].as_ref().unwrap().write(
                    write,
                    (start as u32) + 8 + str_off,
                    &mut str_ptr,
                )?;
            }
        }

        Ok(())
    }
}
