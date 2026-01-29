fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define our search paths
    // Note: these currently point towards the assets folder in the roslibrust repository,
    // you'll want to point this at the location of your own .msg/.srv files
    let p = vec![
        "./assets/common_interfaces".into(),
        "./assets/rcl_interfaces/builtin_interfaces".into(),
    ];

    // Actually invoke code generation on our search paths.
    let (source, dependent_paths) =
        roslibrust::codegen::find_and_generate_ros_messages_without_ros_package_path(p)?;
    // This returns two things:
    // 1) A TokenStream which is the rust code we want to generate
    // 2) A list of paths that if modified would require the code to be regenerated. We use this to inform Cargo
    //    of when to re-run our build script.

    // It is important for build scripts to only output files to OUT_DIR which is an environment variable set by Cargo.
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    // Name of the file in out_dir we want to write our generated code to
    let dest_path = std::path::Path::new(&out_dir).join("messages.rs");
    // Write the generated code to disk
    std::fs::write(dest_path, source.to_string())?;

    // If we stopped at this point, our code would still work, but Cargo would not know to rebuild
    // our package when a message file changed.
    // Cargo recognizes certain command line strings that build scripts print out:
    for path in &dependent_paths {
        // Tell cargo to re-run our build script if any of these files change
        println!("cargo:rerun-if-changed={}", path.display());
    }

    wesl::Wesl::new("src/shaders").build_artifact(&"package::compute_to_render".parse().unwrap(), "compute_to_render");

    Ok(())
}
