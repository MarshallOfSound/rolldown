use oxc::codegen::{self, CodegenOptions, CommentOptions};
use oxc_allocator::AllocatorPool;
use rolldown_common::{MinifyOptions, NormalizedBundlerOptions};
use rolldown_ecmascript::EcmaCompiler;
use rolldown_error::BuildResult;
use rolldown_sourcemap::collapse_sourcemaps;
#[cfg(not(target_family = "wasm"))]
use rolldown_utils::rayon::IndexedParallelIterator;
use rolldown_utils::rayon::{IntoParallelIterator, ParallelIterator};

use crate::type_alias::IndexInstantiatedChunks;

use super::GenerateStage;

impl GenerateStage<'_> {
  #[tracing::instrument(level = "debug", skip_all)]
  pub fn minify_chunks(
    options: &NormalizedBundlerOptions,
    chunks: &mut IndexInstantiatedChunks,
  ) -> BuildResult<()> {
    let (compress, minify_option, remove_whitespace, ascii_only) = match &options.minify {
      MinifyOptions::Disabled => return Ok(()),
      MinifyOptions::DeadCodeEliminationOnly(options) => (false, options, false, false),
      MinifyOptions::Enabled((options, flags)) => {
        (true, options, flags.remove_whitespace, flags.ascii_only)
      }
    };
    let allocator_pool = AllocatorPool::new(rayon::current_num_threads());
    // Largest chunks first, one per task: minification time is roughly linear in chunk size
    // and a multi-megabyte vendor chunk takes seconds, so letting it start last (index order,
    // coarse splits) makes it the tail of the whole stage. Order does not affect output.
    let mut by_size = chunks.iter_mut().collect::<Vec<_>>();
    by_size.sort_by_key(|chunk| std::cmp::Reverse(chunk.content.as_bytes().len()));
    let chunks_largest_first = by_size.into_par_iter();
    // One chunk per rayon task, so the sorted order is the start order (the wasm shim's
    // iterator is sequential and has no splitting to control).
    #[cfg(not(target_family = "wasm"))]
    let chunks_largest_first = chunks_largest_first.with_max_len(1);
    chunks_largest_first.try_for_each(|chunk| -> anyhow::Result<()> {
      if test_d_ts_pattern(chunk.preliminary_filename.as_str()) {
        return Ok(());
      }
      match chunk.kind {
        rolldown_common::InstantiationKind::Ecma(_) => {
          let codegen_options = CodegenOptions {
            minify: remove_whitespace,
            ascii_only,
            comments: CommentOptions {
              normal: !remove_whitespace,
              jsdoc: options.comments.jsdoc && !remove_whitespace,
              annotation: options.comments.annotation && !remove_whitespace,
              legal: if options.comments.legal || !remove_whitespace {
                codegen::LegalComment::Inline
              } else {
                codegen::LegalComment::None
              },
            },
            ..CodegenOptions::default()
          };

          let allocator_guard = allocator_pool.get();
          // The minify map borrows the pre-minify `chunk.content` (as `sourcesContent`,
          // which the collapse discards), so collapse before swapping in the minified
          // content instead of paying an `into_owned` copy of the whole chunk text.
          let (minified_content, collapsed_map) = {
            // TODO: Do we need to ensure `chunk.preliminary_filename` to be absolute path?
            let (minified_content, new_map) = EcmaCompiler::dce_or_minify(
              &allocator_guard,
              chunk.content.try_as_inner_str()?,
              options.format.source_type().with_jsx(true),
              chunk.map.is_some(),
              chunk.preliminary_filename.as_str(),
              compress,
              minify_option.clone(),
              codegen_options,
            );
            let collapsed_map = match (&chunk.map, &new_map) {
              (Some(origin_map), Some(new_map)) => {
                Some(collapse_sourcemaps(&[origin_map, new_map]))
              }
              _ => {
                // TODO: Map is dirty. Should we reset the `chunk.map` to `None`?
                None
              }
            };
            (minified_content, collapsed_map)
          };
          chunk.content = minified_content.into();
          if let Some(map) = collapsed_map {
            chunk.map = Some(map);
          }
        }
        rolldown_common::InstantiationKind::None
        | rolldown_common::InstantiationKind::Sourcemap(_) => {}
      }
      Ok(())
    })?;

    Ok(())
  }
}

fn test_d_ts_pattern(input: &str) -> bool {
  input.ends_with(".d.ts") || input.ends_with(".d.cts") || input.ends_with(".d.mts")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_edge_cases() {
    assert!(test_d_ts_pattern(".d.ts"));
    assert!(test_d_ts_pattern(".d.cts"));
    assert!(test_d_ts_pattern(".d.mts"));
  }

  #[test]
  fn test_invalid_patterns_wrong_extension() {
    assert!(!test_d_ts_pattern(".d.tsx"));
    assert!(!test_d_ts_pattern(".d.ctsx"));
    assert!(!test_d_ts_pattern(".d.mtsx"));
    assert!(!test_d_ts_pattern(".d.cjs"));
  }

  #[test]
  fn test_invalid_patterns_missing_d() {
    assert!(!test_d_ts_pattern(".c.ts"));
    assert!(!test_d_ts_pattern(".m.ts"));
    assert!(!test_d_ts_pattern("abc.ts"));
    assert!(!test_d_ts_pattern("d.ts"));
  }

  #[test]
  fn test_invalid_patterns_extra_chars() {
    assert!(!test_d_ts_pattern(".da.ts"));
    assert!(!test_d_ts_pattern(".d.ats"));
    assert!(!test_d_ts_pattern(".d.tsa"));
  }

  #[test]
  fn test_invalid_patterns_short_input() {
    assert!(!test_d_ts_pattern(".d"));
    assert!(!test_d_ts_pattern(".t"));
    assert!(!test_d_ts_pattern("."));
    assert!(!test_d_ts_pattern(""));
    assert!(!test_d_ts_pattern(".ts")); // added test
  }
}
