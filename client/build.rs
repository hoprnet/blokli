fn main() {
    println!("cargo:rerun-if-changed=./target-api-schema.graphql");

    cynic_codegen::register_schema("blokli")
        .from_sdl_file("./target-api-schema.graphql")
        .unwrap()
        .as_default()
        .unwrap();
}
