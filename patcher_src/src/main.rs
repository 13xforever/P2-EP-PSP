mod lib;
use byteorder::{LittleEndian, ReadBytesExt};
use lib::{cpk::CPK, iso::ISO, util::BinaryStruct};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    prelude::*,
};

use crate::lib::{event::EventArch, iso::ISODirent};

extern "C" {
    pub fn pspDecryptPRX(
        inbuf: *const cty::uint8_t,
        inbuf: *mut cty::uint8_t,
        size: cty::uint32_t,
    ) -> cty::c_int;
}

#[allow(dead_code)]
fn remove_extraneous() -> std::io::Result<()> {
    std::fs::remove_file("iso/PSP_GAME/INSDIR/I020.DAT")?;
    std::fs::remove_file("iso/PSP_GAME/SYSDIR/UPDATE/DATA.BIN")?;
    std::fs::remove_file("iso/PSP_GAME/SYSDIR/UPDATE/EBOOT.BIN")?;
    std::fs::remove_file("iso/PSP_GAME/SYSDIR/BOOT.BIN")?;
    std::fs::remove_file("iso/PSP_GAME/SYSDIR/UPDATE/PARAM.SFO")
}
#[allow(dead_code)]
fn copy_eng() -> std::io::Result<()> {
    std::fs::copy("dist/ENG.BIN", "iso/PSP_GAME/USRDIR/ENG.BIN")?;
    Ok(())
}
#[allow(dead_code)]
fn extract_iso(path: &Path) -> std::io::Result<()> {
    println!("Extracting iso");
    let file = File::open(path)?;
    let mut iso = ISO::new(file);
    iso.extract(Path::new("./iso/"))
}

#[allow(dead_code)]
fn apply_xdelta_patch(src: &Path, patch: &Path) -> std::io::Result<()> {
    println!("Applying patch {}", patch.to_str().unwrap());
    let patch_data = std::fs::read(patch)?;
    let data = std::fs::read(src)?;
    let res = xdelta3::decode(&patch_data, &data);
    match res {
        Some(patched) => std::fs::write(src, &patched),
        None => panic!("Failed to apply patch {}", patch.to_str().unwrap()),
    }
}
#[allow(dead_code)]
fn apply_misc_patches() -> std::io::Result<()> {
    let patches = vec![
        ("iso/PSP_GAME/SYSDIR/", "EBOOT.BIN"),
        ("iso/PSP_GAME/", "PARAM.SFO"),
    ];
    patches.into_iter().try_for_each(|x| {
        let mut path: PathBuf = x.0.into();
        path.push(x.1);
        let mut patch_path = PathBuf::from("dist/");
        patch_path.push(&format!("{}.patch", x.1));
        apply_xdelta_patch(&path, &patch_path)
    })
}
fn decrypt_eboot() -> std::io::Result<()> {
    let eboot = std::fs::read("iso/PSP_GAME/SYSDIR/EBOOT.BIN")?;
    let elf_size = (&eboot[0x28..0x30]).read_u32::<LittleEndian>()?;
    let psp_size = (&eboot[0x2c..0x30]).read_u32::<LittleEndian>()?;
    let size = elf_size.max(psp_size);
    let mut output = vec![0; size as usize];
    let res = unsafe { pspDecryptPRX(eboot.as_ptr(), output.as_mut_ptr(), size) };
    if res < 0 {
        panic!("Unable to decrypt eboot.");
    }
    std::fs::write("iso/PSP_GAME/SYSDIR/EBOOT.BIN", &output[0..res as usize])
}

#[allow(dead_code)]
fn extract_cpk() -> std::io::Result<()> {
    std::fs::create_dir_all("cpk")?;
    let mut file = File::open("iso/PSP_GAME/USRDIR/pack/P2PT_ALL.cpk")?;
    let mut cpk = CPK::read(&mut file)?;
    cpk.map_files(&mut file, |x, y| {
        println!("Extract cpk file [{}]{}", x.id, x.name);
        let patch_path = PathBuf::from(format!("dist/cpk_dist/{}.patch", &x.name));
        let out_data = if patch_path.exists() {
            let patch_data = std::fs::read(&patch_path)?;
            let res = xdelta3::decode(&patch_data, &y);
            match res {
                Some(patched) => patched,
                None => panic!("Failed to apply patch {}", patch_path.to_str().unwrap()),
            }
        } else {
            y
        };
        std::fs::write(format!("cpk/{}.bin", x.id), out_data)
    })
}

#[allow(dead_code)]
fn build_cpk() -> std::io::Result<()> {
    // let mut file = File::open("P2PT_ALL.cpk")?;
    let mut file = File::open("iso/PSP_GAME/USRDIR/pack/P2PT_ALL.cpk")?;
    let mut cpk = CPK::read(&mut file)?;
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open("iso/PSP_GAME/USRDIR/pack/P2PT_ALL.cpk")?;

    cpk.write_cpk(PathBuf::from("cpk/"), &mut out)
}

#[allow(dead_code)]
fn patch_event() -> std::io::Result<()> {
    let data = std::fs::read("cpk/6000.bin")?;
    let mut event: EventArch = EventArch::try_from(data)?;

    event.map_scripts(|name, event| {
        let patch_path = PathBuf::from(format!("dist/event_dist/{}.patch", &name));
        Ok(if patch_path.exists() {
            let patch_data = std::fs::read(&patch_path)?;
            let res = xdelta3::decode(&patch_data, event);
            match res {
                Some(patched) => {
                    // std::fs::write(format!("event/{}.bin", &name), &patched)?;
                    Some(patched)
                }
                None => panic!("Failed to apply patch {}", patch_path.to_str().unwrap()),
            }
        } else {
            None
        })
    })?;

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open("cpk/6000.bin")?;
    let toc = event.write(&mut output)?;

    let mut eboot = OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(false)
        .open("iso/PSP_GAME/SYSDIR/EBOOT.BIN")?;
    eboot.seek(SeekFrom::Start(0x8c570c4 + 0xc0 - 0x8804000))?;
    eboot.write_all(&toc)?;
    // eboot.write_all_at(&toc, 0x8c570c4 + 0xc0 - 0x8804000)?;

    Ok(())

    // dbg!(&event);
    // let file = File::open("cpk/6000.bin")?;

    // Ok(())
}

#[allow(dead_code)]
fn build_iso(path: &Path) -> std::io::Result<()> {
    println!("Building iso... This may take a minute");
    let original = File::open(path)?;
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open("P2EP_EN.iso")?;

    let mut old_iso = ISO::new(original);
    let mut iso = ISO::new(out);
    let pvd = old_iso.get_pvd()?;
    iso.build_from_dir(pvd, PathBuf::from("iso/"))?;

    Ok(())
    // cpk.write_cpk(PathBuf::from("cpk/"), &mut out)
}
fn cleanup() -> std::io::Result<()> {
    std::fs::remove_dir_all("iso")?;
    std::fs::remove_dir_all("cpk")
}
fn main() -> std::io::Result<()> {
    let mut iso_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Please make sure the iso is the first argument."),
    );
    iso_path = iso_path.canonicalize()?;
    // let new_path = std::env::current_exe()?.parent().unwrap();
    std::env::set_current_dir(std::env::current_exe()?.parent().unwrap())?;

    extract_iso(&iso_path)?;
    copy_eng()?;
    remove_extraneous()?;
    decrypt_eboot()?;
    apply_misc_patches()?;
    extract_cpk()?;
    patch_event()?;
    build_cpk()?;
    build_iso(&iso_path)?;
    cleanup()?;
    println!("Done!");

    Ok(())
}
