fn main() {
    tauri_build::build();

    // `cargo test` links `tauri::test`'s mock runtime, whose `Builder` construction reaches
    // the tray/menu code and therefore statically imports `TaskDialogIndirect` from
    // `comctl32.dll`. That export exists only in Common Controls v6, which a process gets
    // from its embedded application manifest; `tauri-build` embeds that manifest for
    // `[[bin]]` targets only, so a test executable aborted at load with
    // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before a single test ran. Integration-test
    // targets get the dependency merged in here. It is declared inline so no file path is
    // involved; shipped binaries are untouched because these are `*-tests` link args.
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
