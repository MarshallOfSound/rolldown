use arcstr::ArcStr;
use oxc_str::CompactStr;
use rolldown_utils::indexmap::FxIndexSet;

use crate::{ExportsKind, ModuleId};

#[derive(Debug)]
pub struct ModuleInfo {
  pub code: Option<ArcStr>,
  pub id: ModuleId,
  pub is_entry: bool,
  pub importers: FxIndexSet<ModuleId>,
  pub dynamic_importers: FxIndexSet<ModuleId>,
  pub imported_ids: FxIndexSet<ModuleId>,
  pub dynamically_imported_ids: FxIndexSet<ModuleId>,
  pub exports: Vec<CompactStr>,
  pub input_format: ExportsKind,
  /// The module uses `await` (or `for await` / `await using`) at the top level. Only known once
  /// the module is parsed; `false` for external modules and before parsing.
  pub has_top_level_await: bool,
}
