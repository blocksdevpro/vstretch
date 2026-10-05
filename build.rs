fn main() {
    embed_resource::compile("assets/vstretch.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
