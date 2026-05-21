fn main() {
    println!("cargo:rustc-link-search=native=target/debug");
    println!("cargo:rustc-link-lib=fs4me_local");

    // Настраиваем RPATH: указываем, где искать .so при запуске бинарника
    // $ORIGIN означает «в той же директории, где находится бинарник»
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
}
