fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=styles.css");
    println!("cargo:rerun-if-env-changed=TAILWIND_CLI");
    let config = topcoat::tailwind::BuildConfig::new()
        .input("styles.css")
        .version("4.3.2");
    if std::env::var_os("TAILWIND_CLI").is_some() {
        config.executable_env("TAILWIND_CLI").render()
    } else {
        config.render()
    }
    .expect("compile panel styles with Tailwind");
}
