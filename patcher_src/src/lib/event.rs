use std::io::prelude::*;
use std::io::Cursor;

use byteorder::WriteBytesExt;
use byteorder::{LittleEndian, ReadBytesExt};

use flate2::read::GzDecoder;
use flate2::Compression;
use flate2::GzBuilder;
use flate2::GzHeader;

use super::util::read_bytes_at;
use super::util::write_bytes_at;

pub struct Event {
    pub name: String,
    pub header: GzHeader,
    // name: String,
    pub contents: Vec<u8>,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
            .field("name", &self.name)
            .field("count", &self.contents.len())
            .finish()
    }
}

impl TryFrom<Vec<u8>> for Event {
    type Error = std::io::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let mut stream = GzDecoder::new(&value[..]);
        let header = stream.header().unwrap().clone();
        let name = String::from_utf8(header.filename().unwrap().to_vec()).unwrap();
        println!("Decompressing event {}", &name);
        let mut contents = Vec::new();
        stream.read_to_end(&mut contents)?;

        Ok(Self {
            header,
            name,
            contents,
        })
    }
}
impl TryInto<Vec<u8>> for Event {
    type Error = std::io::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut builder = GzBuilder::new();
        if let Some(dat) = self.header.filename() {
            builder = builder.filename(dat);
        }
        if let Some(dat) = self.header.comment() {
            builder = builder.comment(dat);
        }
        if let Some(dat) = self.header.extra() {
            builder = builder.extra(dat);
        }
        builder = builder.operating_system(self.header.operating_system());
        builder = builder.mtime(self.header.mtime());

        println!("Compressing event {}", &self.name);
        let mut writer = builder.write(Vec::new(), Compression::default());
        writer.write_all(&self.contents)?;
        writer.finish()
    }
}

#[derive(Debug)]
pub struct EventArch {
    pub events: Vec<Event>,
}

impl TryFrom<Vec<u8>> for EventArch {
    type Error = std::io::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let mut read = Cursor::new(&value);
        let mut events = Vec::new();

        loop {
            let start = read.read_u32::<LittleEndian>()?;
            let end = read.read_u32::<LittleEndian>()?;
            if start == 0 {
                break;
            }
            let buff = read_bytes_at(&mut read, start, end - start)?;
            events.push(Event::try_from(buff)?);
        }

        Ok(Self { events })
    }
}

impl EventArch {
    pub fn write<W: Write + Seek>(self, write: &mut W) -> std::io::Result<Vec<u8>> {
        let header_size = 8 * (self.events.len() as u32);

        let compressed = self
            .events
            .into_iter()
            .map::<std::io::Result<Vec<u8>>, _>(|x| x.try_into());

        let mut toc: Vec<u8> = Vec::new();
        let start = (header_size + 0x7ff) & !0x7ff;
        let mut ptr = start;

        for buff in compressed {
            let buff = buff?;
            toc.write_u32::<LittleEndian>(ptr)?;
            write.write_u32::<LittleEndian>(ptr)?;
            write_bytes_at(write, ptr, &(buff))?;
            ptr += ((buff.len() as u32) + 0x7ff) & !0x7ff;
            write.write_u32::<LittleEndian>(ptr)?;
            toc.write_u32::<LittleEndian>(ptr)?;
        }
        Ok(toc)
    }
    pub fn map_scripts<F>(&mut self, func: F) -> std::io::Result<()>
    where
        F: Fn(&str, &Vec<u8>) -> std::io::Result<Option<Vec<u8>>>,
    {
        let len = self.events.len();
        for i in 0..len {
            match func(&self.events[i].name, &self.events[i].contents)? {
                Some(data) => self.events[i].contents = data,
                None => (),
            }
        }
        Ok(())
    }
}
