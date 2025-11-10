
fn main() {

	let config = slint_build::CompilerConfiguration::new()
		.embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);
	slint_build::compile_with_config("./autoplant_userinterface/main.slint", config).unwrap();
	slint_build::print_rustc_flags().unwrap();
	mem()
}


use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn mem() {
	// Put `memory.x` in our output directory and ensure it's
	// on the linker search path.
	let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
	File::create(out.join("memory.x")).unwrap().write_all(include_bytes!("memory.x")).unwrap();
	println!("cargo:rustc-link-search={}", out.display());

	// By default, Cargo will re-run a build script whenever
	// any file in the project changes. By specifying `memory.x`
	// here, we ensure the build script is only re-run when
	// `memory.x` is changed.
	println!("cargo:rerun-if-changed=memory.x");

	println!("cargo:rustc-link-arg-bins=--nmagic");
	println!("cargo:rustc-link-arg-bins=-Tlink.x");
	println!("cargo:rustc-link-arg-bins=-Tlink-rp.x");
	println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}