extern crate cc;

fn main() {
    cc::Build::new()
        .file("c_src/libkirk/AES.c")
        .file("c_src/libkirk/amctrl.c")
        .file("c_src/libkirk/bn.c")
        .file("c_src/libkirk/ec.c")
        .file("c_src/libkirk/kirk_engine.c")
        .file("c_src/libkirk/SHA1.c")
        .file("c_src/PrxDecrypter.cpp")
        .compile("lib");
}
