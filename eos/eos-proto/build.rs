fn main() -> Result<(), Box<dyn std::error::Error>> {
    capnpc::CompilerCommand::new()
        .file("schema/eos.capnp")
        .run()?;
    Ok(())
}
