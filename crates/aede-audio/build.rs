fn main() {
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=vendor");
    #[cfg(feature = "decode")]
    compile_codecs();
}

#[cfg(feature = "decode")]
fn compile_codecs() {
    let root = std::path::Path::new("vendor");
    let wavpack = root.join("wavpack-5.9.0");
    let speex = root.join("speex-1.2.1");
    let musepack = root.join("musepack_src_r475");
    let mut c = cc::Build::new();
    c.std("c11")
        .warnings(false)
        .include(wavpack.join("include"))
        .include(speex.join("include"))
        .include(musepack.join("include"))
        .include("native")
        .define("FLOATING_POINT", "1")
        .define("EXPORT", "")
        .define("SPEEX_VERSION", "\"1.2.1\"")
        .define("SPEEX_MAJOR_VERSION", "1")
        .define("SPEEX_MINOR_VERSION", "2")
        .define("SPEEX_MICRO_VERSION", "1")
        .define("SPEEX_EXTRA_VERSION", "\"\"")
        .define("LONG_SEEK_TABLE", "1")
        .define("_CRT_SECURE_NO_WARNINGS", "1");
    // WavPack's integer predictor arithmetic needs modulo signed add/multiply,
    // including while rejecting corrupt packets. Keep that compiler contract
    // confined to WavPack; other codecs retain ordinary overflow diagnostics.
    let mut wavpack_build = c.clone();
    wavpack_build.flag_if_supported("-fwrapv");
    for name in [
        "common_utils",
        "decorr_utils",
        "entropy_utils",
        "extra1",
        "extra2",
        "open_utils",
        "open_filename",
        "open_legacy",
        "open_raw",
        "pack",
        "pack_dns",
        "pack_floats",
        "pack_utils",
        "read_words",
        "tags",
        "tag_utils",
        "unpack",
        "unpack_floats",
        "unpack_seek",
        "unpack_utils",
        "write_words",
    ] {
        wavpack_build.file(wavpack.join("src").join(format!("{name}.c")));
    }
    wavpack_build.file("native/wavpack.c");
    wavpack_build.compile("aede_wavpack");
    for name in [
        "cb_search",
        "exc_10_32_table",
        "exc_8_128_table",
        "filters",
        "gain_table",
        "hexc_table",
        "high_lsp_tables",
        "lsp",
        "ltp",
        "speex",
        "stereo",
        "vbr",
        "vq",
        "bits",
        "exc_10_16_table",
        "exc_20_32_table",
        "exc_5_256_table",
        "exc_5_64_table",
        "gain_table_lbr",
        "hexc_10_32_table",
        "lpc",
        "lsp_tables_nb",
        "modes",
        "modes_wb",
        "nb_celp",
        "quant_lsp",
        "sb_celp",
        "speex_callbacks",
        "speex_header",
        "window",
    ] {
        c.file(speex.join("libspeex").join(format!("{name}.c")));
    }
    for name in [
        "huffman",
        "mpc_decoder",
        "requant",
        "streaminfo",
        "synth_filter",
        "mpc_bits_reader",
        "mpc_demux",
    ] {
        c.file(musepack.join("libmpcdec").join(format!("{name}.c")));
    }
    c.file(musepack.join("common/crc32.c"));
    c.file(musepack.join("common/huffman-bcl.c"));
    c.file("native/musepack.c");
    c.file("native/speex.c");
    // Generic C paths, no optional assembly, codec threads, DSP or downloads.
    c.compile("aede_native_codecs");
    if !std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|s| s == "windows") {
        println!("cargo:rustc-link-lib=m");
    }
}
