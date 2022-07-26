use std::collections::VecDeque;
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::{prelude::*, SeekFrom};
use std::path::{Path, PathBuf};

use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use byteorder::{BigEndian, ByteOrder, LittleEndian};

use crate::align;
use crate::lib::util::write_string_pad;

use super::dirent::{self, DirEnt};
// use super::endian::*;
use super::pvd::*;
use super::util::{write_string, BinaryStruct};

#[derive(Debug)]
pub struct ISO {
    fp: File,
}

// enum ISODirentType {
//     Directory(Vec<Box<ISODirent>>)
// }

const WRITE_FILES: bool = true;

const BLOCK_SIZE: usize = 4 * 1048 * 1024;
pub struct ISODirent {
    pub is_dir: bool,
    pub dirent: DirEnt,
    pub path: PathBuf,
    pub children: Vec<Box<ISODirent>>,
}
impl ISODirent {
    pub fn set_xa(&mut self, xa: bool) {
        self.dirent.set_xa(xa);
        self.children.iter_mut().for_each(|x| x.set_xa(xa));
    }
    pub fn update_dirsize(&mut self) {
        if self.is_dir {
            let size = self.children.iter_mut().fold(0, |p, x| {
                x.update_dirsize();
                p + x.dirent.length as u32
            });
            // self.dirent.size = size + (48) * 2;
            self.dirent.size = 2048;
        }
        // self.dirent.set_xa(xa);
        // self.children.iter_mut().for_each(|x| x.set_xa(xa));
    }
    pub fn allocate_dir_sectors(&mut self, next: u32) -> u32 {
        if self.is_dir {
            self.dirent.sector = next;
            self.children
                .iter_mut()
                .fold(next + 1, |curr, child| child.allocate_dir_sectors(curr))
        } else {
            next
        }
    }
    pub fn allocate_file_sectors(&mut self, next: u32) -> u32 {
        if self.is_dir {
            self.children
                .iter_mut()
                .fold(next, |curr, child| child.allocate_file_sectors(curr))
        } else {
            self.dirent.sector = next;
            next + (self.dirent.size + 0x7ff) / 2048
        }
    }
    fn write_path_impl<W: Write + Seek, B: ByteOrder>(
        &self,
        write: &mut W,
        id: u16,
        parent: u16,
        first: bool,
    ) -> std::io::Result<u16> {
        if self.is_dir {
            if first {
                write.write_u8(1)?;
            } else {
                write.write_u8(self.dirent.name.len() as u8)?;
            }
            write.write_u8(0)?;
            write.write_u32::<B>(self.dirent.sector)?;
            write.write_u16::<B>(parent)?;

            if first {
                write.write_u8(0)?;
                write.write_u8(0)?;
            } else {
                write_string(write, &self.dirent.name)?;
                if (self.dirent.name.len() & 1) == 1 {
                    write.write_u8(0)?;
                }
            }
            self.children.iter().try_fold(id + 1, |curr, child| {
                child.write_path_impl::<W, B>(write, curr, id, false)
            })
        } else {
            Ok(id)
        }
    }
    fn calculate_path_table_size(&self) -> usize {
        if self.is_dir {
            self.children
                .iter()
                .fold(align!(self.dirent.name.len() + 8 + 1, 2), |curr, child| {
                    curr + child.calculate_path_table_size()
                })
        } else {
            0
        }
    }
    pub fn write_path_table<W: Write + Seek, B: ByteOrder>(
        &self,
        write: &mut W,
        sector: u32,
    ) -> std::io::Result<usize> {
        write.seek(SeekFrom::Start((sector as u64) * 2048))?;
        let start = write.stream_position()?;
        let mut q = VecDeque::new();
        let mut idx = 1;
        q.push_back((1, self));
        while let Some((parent, ent)) = q.pop_front() {
            let me = idx;
            idx += 1;
            if me == 1 {
                write.write_u8(1)?;
            } else {
                write.write_u8(ent.dirent.name.len() as u8)?;
            }
            write.write_u8(0)?;
            write.write_u32::<B>(ent.dirent.sector)?;
            write.write_u16::<B>(parent)?;
            if me == 1 {
                write.write_u8(0)?;
                write.write_u8(0)?;
            } else {
                write_string(write, &ent.dirent.name)?;
                if (ent.dirent.name.len() & 1) == 1 {
                    write.write_u8(0)?;
                }
            }
            for child in ent.children.iter() {
                if child.is_dir {
                    q.push_back((me, &*child))
                }
            }
        }
        let end = write.stream_position()?;
        Ok((end - start) as usize)
    }

    pub fn write<W: Write + Seek>(&self, write: &mut W, parent: &DirEnt) -> std::io::Result<()> {
        let curr = write.stream_position()?;
        write.seek(SeekFrom::Start((self.dirent.sector as u64) * 2048))?;
        if self.is_dir {
            let mut dot = self.dirent.clone();
            dot.name = String::from("\x00");
            dot.update_length();
            dot.write(write)?;

            let mut dotdot = parent.clone();
            dotdot.name = String::from("\x01");
            dotdot.update_length();
            dotdot.write(write)?;

            for child in self.children.iter() {
                child.dirent.write(write)?;
                child.write(write, &self.dirent)?;
            }
        } else {
            if WRITE_FILES {
                let mut file = File::open(&self.path)?;
                let mut buff = vec![0u8; BLOCK_SIZE];
                let mut len = self.dirent.size as usize;
                while len > BLOCK_SIZE {
                    file.read_exact(buff.as_mut_slice())?;
                    write.write_all(&buff)?;
                    len -= BLOCK_SIZE;
                }
                file.read_exact(&mut buff[0..len])?;
                write.write_all(&buff[0..len])?;
            }
        }
        write.seek(SeekFrom::Start(curr))?;
        Ok(())
    }
}
impl Debug for ISODirent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ISODirent")
            // .field("is_dir", &self.is_dir)
            .field("path", &self.path)
            .field("dirent", &self.dirent)
            .field("children", &self.children)
            .finish()
    }
}

impl TryFrom<PathBuf> for Box<ISODirent> {
    type Error = std::io::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        let mut children = Vec::new();
        let dirent = DirEnt::try_from(path.as_ref())?;
        let is_dir = path.is_dir();
        // let dirent =

        if is_dir {
            for ent in path.read_dir().expect("Failed to read directory") {
                match ent {
                    Ok(ent) => {
                        children.push(ent.path().try_into()?);
                    }
                    Err(_) => panic!("Faild to read"),
                }
            }
        }
        Ok(Self::new(ISODirent {
            is_dir,
            dirent,
            path,
            children,
        }))
    }
}

// impl ISODirent {
//     pub fn parse_dir(path: PathBuf) -> std::io::Result<Box<Self>> {
//         let files = path.read_dir()?;
//         todo!()
//     }
// }

impl ISO {
    pub fn new(fp: File) -> Self {
        Self { fp: fp }
    }

    pub fn build_from_dir(&mut self, mut pvd: Box<PVD>, path: PathBuf) -> std::io::Result<()> {
        let mut sector = [0x20u8; 2048];

        self.write_sector(14, &sector)?;
        self.write_sector(15, &sector)?;
        sector.fill(0);
        sector[0] = 0xff;
        sector[1..6].copy_from_slice("CD001".as_bytes());
        sector[6] = 1;
        self.write_sector(17, &sector)?;

        let mut root: Box<ISODirent> = (path).try_into()?;
        root.set_xa(true);
        root.update_dirsize();
        let file_sector = root.allocate_dir_sectors(22);
        let last_sector = root.allocate_file_sectors(file_sector);
        let size = root.write_path_table::<_, LittleEndian>(&mut self.fp, pvd.l_sector)?;
        pvd.path_table_size = size as u32;
        root.write_path_table::<_, LittleEndian>(&mut self.fp, pvd.l_sector_opt)?;
        root.write_path_table::<_, BigEndian>(&mut self.fp, pvd.m_sector)?;
        root.write_path_table::<_, BigEndian>(&mut self.fp, pvd.m_sector_opt)?;
        pvd.volume_space_size = align!(last_sector, 16);
        pvd.root_ent = Box::new(root.dirent.clone());
        pvd.root_ent.set_xa(false);
        pvd.root_ent.name = String::from("\x00");
        pvd.root_ent.update_length();

        self.fp.seek(SeekFrom::Start(2048 * 16))?;
        pvd.write(&mut self.fp)?;
        self.fp.write_u8(0)?;
        write_string_pad(&mut self.fp, "NPJH-50581|4185637DD632EE5C|0001", 0x8d)?;
        write_string_pad(&mut self.fp, "CD-XA001", 0x173)?;

        root.write(&mut self.fp, &root.dirent)?;
        if WRITE_FILES {
            if pvd.volume_space_size > last_sector {
                sector.fill(0);
                self.write_sector((pvd.volume_space_size - 1) as u64, &sector)?;
            }
        }

        Ok(())
    }
    pub fn read_sector(&mut self, sector: u64, buff: &mut [u8; 2048]) -> std::io::Result<()> {
        self.fp.seek(std::io::SeekFrom::Start(sector * 2048))?;
        self.fp.read_exact(buff)
    }
    pub fn write_sector(&mut self, sector: u64, buff: &[u8; 2048]) -> std::io::Result<()> {
        self.fp.seek(std::io::SeekFrom::Start(sector * 2048))?;
        self.fp.write_all(buff)
    }

    fn extract_dir(&mut self, path: &Path, ent: &Box<DirEnt>) -> std::io::Result<()> {
        let children = self.read_dir_ents(ent.sector as u64)?;
        for child in children.into_iter().skip(2) {
            if (child.flags & 2) == 0 {
                //file
                self.extract_file(path, &child)?;
            } else {
                let mut new_path = path.to_path_buf();
                new_path.push(&child.name);
                std::fs::create_dir_all(&new_path)?;
                self.extract_dir(&new_path, &child)?;
            }
        }
        Ok(())
    }
    fn extract_file(&mut self, dir: &Path, ent: &Box<DirEnt>) -> std::io::Result<()> {
        let mut path = dir.to_path_buf();
        path.push(&ent.name);
        println!("Extracting {}", &path.to_str().unwrap());
        let mut out = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        let sector = ent.sector as u64;
        let mut buff = vec![0u8; BLOCK_SIZE];

        self.fp.seek(SeekFrom::Start(sector * 2048))?;
        let mut len = ent.size as usize;
        while len > BLOCK_SIZE {
            self.fp.read_exact(buff.as_mut_slice())?;
            out.write_all(&buff)?;
            len -= BLOCK_SIZE;
        }
        self.fp.read_exact(&mut buff[0..len])?;
        out.write_all(&buff[0..len])?;

        out.flush()
    }
    pub fn extract(&mut self, folder: &Path) -> std::io::Result<()> {
        let pvd = self.get_pvd()?;
        self.extract_dir(folder, &pvd.root_ent)
    }
    pub fn read_dir_ents(&mut self, sector: u64) -> std::io::Result<Vec<Box<DirEnt>>> {
        let mut ents = Vec::new();
        self.fp.seek(SeekFrom::Start(sector * 2048))?;
        loop {
            if self.fp.read_u8()? == 0 {
                break;
            }
            self.fp.seek(SeekFrom::Current(-1))?;
            ents.push(DirEnt::read(&mut self.fp)?);
        }

        Ok(ents)
    }
    // pub fn print_dir(&mut self) -> std::io::Result<()> {
    //     let pvd = self.get_pvd()?;

    //     let mut q: VecDeque<(String, Box<DirEnt>)> = VecDeque::new();
    //     q.push_back(("".into(), pvd.root_ent));
    //     while !q.is_empty() {
    //         let ent = q.pop_front().unwrap();
    //         if ent.1.flags & 2 != 0 {
    //             let children = self.read_dir_ents(ent.1.sector as u64)?;
    //             children
    //                 .into_iter()
    //                 .skip(2)
    //                 .map(|x| (format!("{}{}/", ent.0, ent.1.name), x))
    //                 .for_each(|x| q.push_back(x))
    //         } else {
    //             println!("{}{}", ent.0, ent.1.name);
    //         }
    //     }
    //     Ok(())
    // }
    pub fn get_pvd(&mut self) -> std::io::Result<Box<PVD>> {
        // let pvd: PVD;
        // let mut sector: [u8; 2048] = [0; 2048];
        // self.read_sector(16, &mut sector)?;
        // let mut cursor = Cursor::new(sector);
        self.fp.seek(SeekFrom::Start(16 * 2048))?;
        PVD::read(&mut self.fp)
    }
}
