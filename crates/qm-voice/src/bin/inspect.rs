//! Print an ONNX model's inputs, outputs and metadata. `inspect model.onnx`
use tract_onnx::prelude::*;

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let proto = tract_onnx::onnx().proto_model_for_path(&path)?;
        println!("== {path}");
        for p in &proto.metadata_props {
            println!("  meta {} = {}", p.key, p.value);
        }
        let model = tract_onnx::onnx().model_for_proto_model(&proto)?;
        for (i, o) in model.input_outlets()?.iter().enumerate() {
            println!("  input {i}: {:?}", model.outlet_fact(*o)?);
        }
        for (i, o) in model.output_outlets()?.iter().enumerate() {
            println!("  output {i}: {:?}", model.outlet_fact(*o)?);
        }
    }
    Ok(())
}
