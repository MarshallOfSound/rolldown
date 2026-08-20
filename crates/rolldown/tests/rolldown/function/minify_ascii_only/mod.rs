use rolldown::{BundlerOptions, InputItem, RawMinifyOptions, RawMinifyOptionsDetailed};
use rolldown_testing::{manual_integration_test, test_config::TestMeta};

/// `output.minify.codegen.asciiOnly`: the emitted chunk must be 7-bit clean and still
/// execute with the same semantics (the fixture asserts on itself at runtime).
#[tokio::test(flavor = "multi_thread")]
async fn minify_ascii_only() {
  manual_integration_test!()
    .build(TestMeta { expect_executed: true, ..Default::default() })
    .run(BundlerOptions {
      input: Some(vec![InputItem { name: Some("main".to_string()), import: "./main.js".to_string() }]),
      minify: Some(RawMinifyOptions::Object(RawMinifyOptionsDetailed {
        mangle: Some(Default::default()),
        compress: None,
        remove_whitespace: true,
        ascii_only: true,
      })),
      ..Default::default()
    })
    .await;
  let out = std::fs::read_to_string(
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("tests/rolldown/function/minify_ascii_only/dist/main.js"),
  )
  .expect("dist/main.js");
  assert!(out.is_ascii(), "non-ASCII byte in ascii_only output: {out}");
}
