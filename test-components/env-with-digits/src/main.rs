// Tests edge case #15: Env var names containing digits.
fn main() {
    let s3 = std::env::var("AWS_S3_BUCKET").unwrap_or_default();
    let oauth = std::env::var("OAUTH2_CLIENT_ID").unwrap_or_default();
    let region = std::env::var("AWS_S3_REGION").unwrap_or_default();
    let v2 = std::env::var("API_V2_KEY").unwrap_or_default();
    println!("{s3} {oauth} {region} {v2}");
}
