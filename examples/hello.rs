use uuid::Uuid;

fn main() {
    println!("asdf");
    println!(
        "Generated UUID v7: {}",
        Uuid::now_v7().to_string().to_uppercase()
    );
}
