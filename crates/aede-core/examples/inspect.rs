use std::path::Path;
fn main() {
    for arg in std::env::args().skip(1) {
        let p = Path::new(&arg);
        println!("=== {}", arg);
        match aede_core::tags::read(p) {
            Ok(t) => {
                println!("  props: {:?}", t.properties);
                println!("  art: {}", t.has_embedded_art);
                for (k, v) in &t.fields {
                    println!("  {k} = {v:?}");
                }
            }
            Err(e) => println!("  ERROR: {e}"),
        }
    }
}
