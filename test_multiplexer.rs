// Test script to verify that multiplexer integration works
use ah_tui_multiplexer::default_multiplexer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Try to get a default multiplexer
    match default_multiplexer() {
        Ok(mux) => {
            println!("✅ Successfully created default multiplexer: {}", mux.id());
            println!("✅ Multiplexer is available: {}", mux.is_available());
        }
        Err(e) => {
            println!("❌ Failed to create default multiplexer: {}", e);
        }
    }
    Ok(())
}
