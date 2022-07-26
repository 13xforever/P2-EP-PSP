use std::ops::ControlFlow;
use std::path::PathBuf;
use std::{collections::HashMap, io::SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::lib::util::{
    read_bytes_at, read_string_n, write_bytes_at, write_string, write_string_at, BinaryStruct,
};

use super::decompress::crilayla_decompress;
use super::utf::{UTFDataType, UTFStorage, UTFValue, UTF};

#[derive(Debug)]
pub struct CPK {
    utfs: HashMap<String, Box<UTF>>,
}

#[derive(Debug)]
pub struct CPKFile {
    pub id: u32,
    pub name: String,
    pub file_size: u32,
    pub extract_size: u32,
    pub offset: u32,
}

fn read_utfpacket<R: std::io::Read + std::io::Seek>(
    read: &mut R,
    expected: &str,
) -> std::io::Result<(u64, Box<UTF>)> {
    let name = read_string_n(read, 4)?;
    if name != expected {
        panic!("unexpected UTF packet {}", name);
    }
    let unk = read.read_u32::<LittleEndian>()?;
    assert_eq!(unk, 255);
    let size = read.read_u64::<LittleEndian>()?;
    let utf = UTF::read(read)?;
    Ok((size, utf))
}

// struct CPKWriteFile {
//     pub id: u32,
//     pub name: String,
//     pub size: u32,
//     pub offset: u32
// }
impl CPK {
    pub fn write_cpk<W: std::io::Write + std::io::Seek>(
        &mut self,
        dir: PathBuf,
        write: &mut W,
    ) -> std::io::Result<()> {
        let files: Vec<std::fs::DirEntry> = dir
            .read_dir()?
            .collect::<std::io::Result<Vec<std::fs::DirEntry>>>()?;
        let mut file_map = HashMap::new();
        let mut content_size = 0;
        let mut padded_size = 0;
        files.iter().try_for_each(|file| {
            let name = String::from(file.file_name().to_str().unwrap());
            let end = name.find('.').unwrap();
            let id = name[0..end].parse::<u32>().unwrap();
            let size = file.metadata()?.len() as u32;
            content_size += size;
            padded_size += (size + 0x7ff) & !0x7ff;
            file_map.insert(id, size);
            Ok(()) as std::io::Result<()>
        })?;

        {
            let cpk_header = self.utfs.get_mut("CpkHeader").unwrap();
            cpk_header.get_col_mut("Groups").storage = UTFStorage::ZERO;

            // "ContentOffset": 16384,
            // "ContentSize": 264429568,
            // "TocOffset": 2048,
            // "TocSize": 6680,
            // "ItocOffset": 10240,
            // "ItocSize": 1432,
            // "EnabledPackedSize": 558080966,
            // "EnabledDataSize": 543332720,
            // "Files": 167,
            // "Groups": 1,
            // "Attrs": 1,
            // "Revision": 0,
            // "Sorted": 1,
            // "EID": 1,
            // "CpkMode": 5,

            cpk_header.remove_column("EtocOffset");
            cpk_header.remove_column("EtocSize");
            cpk_header.remove_column("TocCrc");
            cpk_header.remove_column("ItocCrc");
            cpk_header.remove_column("GtocOffset");
            cpk_header.remove_column("GtocSize");
            cpk_header.remove_column("GtocCrc");
            cpk_header.add_col(
                "EnableTocCrc".into(),
                UTFDataType::U16,
                UTFStorage::PER_ROW,
                Some(UTFValue::U16(0)),
            );
            cpk_header.add_col(
                "EnableFileCrc".into(),
                UTFDataType::U16,
                UTFStorage::PER_ROW,
                Some(UTFValue::U16(0)),
            );
        }

        let header_len = self.utfs.get_mut("CpkHeader").unwrap().calculate_size();
        let toc_len = self.utfs.get_mut("TOC").unwrap().calculate_size();
        let itoc_len = self.utfs.get_mut("ITOC").unwrap().calculate_size();

        let content_offset =
            align!(header_len, 2048) + align!(toc_len, 2048) + align!(itoc_len, 2048);
        let mut current_off = content_offset;

        self.utfs
            .get_mut("TOC")
            .unwrap()
            .rows
            .iter_mut()
            .for_each(|x| {
                let id: u32 = x["ID"].as_ref().unwrap().into();
                x.insert(String::from("FileSize"), Some(UTFValue::U32(file_map[&id])));
                x.insert(
                    String::from("ExtractSize"),
                    Some(UTFValue::U32(file_map[&id])),
                );
                x.insert(
                    String::from("FileOffset"),
                    Some(UTFValue::U64((current_off - 0x800) as u64)),
                );
                current_off += align!(file_map[&id] as usize, 2048);
            });
        {
            let cpk_header = self.utfs.get_mut("CpkHeader").unwrap();
            *cpk_header.rows[0].get_mut("ContentOffset").unwrap() =
                Some(UTFValue::U64(content_offset as u64));
            *cpk_header.rows[0].get_mut("ContentSize").unwrap() =
                Some(UTFValue::U64(padded_size as u64));
            *cpk_header.rows[0].get_mut("EnabledPackedSize").unwrap() =
                Some(UTFValue::U64(content_size as u64));
            *cpk_header.rows[0].get_mut("EnabledDataSize").unwrap() =
                Some(UTFValue::U64(content_size as u64));
            *cpk_header.rows[0].get_mut("TocSize").unwrap() = Some(UTFValue::U64(toc_len as u64));
            *cpk_header.rows[0].get_mut("ItocSize").unwrap() = Some(UTFValue::U64(itoc_len as u64));
            *cpk_header.rows[0].get_mut("TocOffset").unwrap() =
                Some(UTFValue::U64(align!(header_len, 2048) as u64));
            *cpk_header.rows[0].get_mut("ItocOffset").unwrap() = Some(UTFValue::U64(
                align!(header_len, 2048) as u64 + align!(toc_len, 2048) as u64,
            ));
            *cpk_header.rows[0].get_mut("Groups").unwrap() = Some(UTFValue::U64(0));
        }
        // *cpk_header.rows[0].get_mut("Groups").unwrap() = Some(UTFValue::U32(0));

        write_string(write, "CPK ")?;
        write.write_u32::<LittleEndian>(0xff)?;
        write.write_u64::<LittleEndian>(header_len as u64)?;
        self.utfs["CpkHeader"].write(write)?;
        write.seek(SeekFrom::Start(2048 - 6))?;
        write_string(write, "(c)CRI")?;

        write_string(write, "TOC ")?;
        write.write_u32::<LittleEndian>(0xff)?;
        write.write_u64::<LittleEndian>(toc_len as u64)?;
        self.utfs["TOC"].write(write)?;

        write.seek(SeekFrom::Start(
            align!(header_len, 2048) as u64 + align!(toc_len, 2048) as u64,
        ))?;
        write_string(write, "ITOC")?;
        write.write_u32::<LittleEndian>(0xff)?;
        write.write_u64::<LittleEndian>(itoc_len as u64)?;
        self.utfs["ITOC"].write(write)?;

        for row in self.utfs["TOC"].rows.iter() {
            let mut path = dir.clone();
            let id: u32 = row["ID"].as_ref().unwrap().into();
            let off: u32 = row["FileOffset"].as_ref().unwrap().into();
            path.push(format!("{}.bin", id));
            let buff = std::fs::read(path)?;
            write_bytes_at(write, (0x800 as u32) + off, &buff)?;
        }

        // todo!()
        Ok(())
    }

    pub fn map_files<F, R: std::io::Read + std::io::Seek>(
        &mut self,
        read: &mut R,
        func: F,
    ) -> std::io::Result<()>
    where
        F: Fn(CPKFile, Vec<u8>) -> std::io::Result<()>,
    {
        let content: u32 = self.utfs["CpkHeader"].rows[0]["TocOffset"]
            .as_ref()
            .unwrap()
            .into();
        let num_files = self.utfs["TOC"].rows.len();
        for ind in 0..num_files {
            let off: u32 = self.utfs["TOC"].rows[ind]["FileOffset"]
                .as_ref()
                .unwrap()
                .into();
            let file = CPKFile {
                id: self.utfs["TOC"].rows[ind]["ID"].as_ref().unwrap().into(),
                name: self.utfs["TOC"].rows[ind]["FileName"]
                    .as_ref()
                    .unwrap()
                    .into(),
                file_size: self.utfs["TOC"].rows[ind]["FileSize"]
                    .as_ref()
                    .unwrap()
                    .into(),
                extract_size: self.utfs["TOC"].rows[ind]["ExtractSize"]
                    .as_ref()
                    .unwrap()
                    .into(),
                offset: content + off,
            };
            let mut data = read_bytes_at(read, file.offset, file.file_size)?;
            if file.extract_size != file.file_size {
                data = crilayla_decompress(data);
            }
            func(file, data)?;
        }
        Ok(())
    }
}
impl BinaryStruct for CPK {
    fn read<R: std::io::Read + std::io::Seek>(read: &mut R) -> std::io::Result<Box<Self>> {
        let mut cpk = Box::new(Self {
            utfs: HashMap::new(),
        });
        // let cpk =
        let header = read_utfpacket(read, "CPK ")?.1;

        let toc_off: u32 = (&header.rows[0]["TocOffset"]).as_ref().unwrap().into();
        let itoc_off: u32 = (&header.rows[0]["ItocOffset"]).as_ref().unwrap().into();

        read.seek(SeekFrom::Start(toc_off as u64))?;
        let toc = read_utfpacket(read, "TOC ")?.1;
        read.seek(SeekFrom::Start(itoc_off as u64))?;
        let itoc = read_utfpacket(read, "ITOC")?.1;

        cpk.utfs.insert("CpkHeader".into(), header);
        cpk.utfs.insert("TOC".into(), toc);
        cpk.utfs.insert("ITOC".into(), itoc);
        Ok(cpk)
    }
}
