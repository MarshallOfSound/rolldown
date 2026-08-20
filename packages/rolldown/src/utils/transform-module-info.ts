import type { ModuleOptions } from '..';
import type { BindingModuleInfo } from '../binding.cjs';
import type { ModuleInfo } from '../types/module-info';
import { unsupported } from './misc';

/**
 * Each `BindingModuleInfo` list field is a binding getter that materializes a fresh JS array of
 * strings, and plugins that walk the module graph call `getModuleInfo` for every module while
 * reading only one or two fields. So the id lists are read from the binding on first access and
 * memoized, rather than all converted up front.
 */
function memo<T>(read: () => T): () => T {
  let loaded = false;
  let value: T;
  return () => {
    if (!loaded) {
      value = read();
      loaded = true;
    }
    return value!;
  };
}

export function transformModuleInfo(info: BindingModuleInfo, option: ModuleOptions): ModuleInfo {
  const importers = memo(() => info.importers);
  const dynamicImporters = memo(() => info.dynamicImporters);
  const importedIds = memo(() => info.importedIds);
  const dynamicallyImportedIds = memo(() => info.dynamicallyImportedIds);
  const exports = memo(() => info.exports);
  return {
    get ast() {
      return unsupported('ModuleInfo#ast');
    },
    get code() {
      return info.code;
    },
    id: info.id,
    get importers() {
      return importers();
    },
    get dynamicImporters() {
      return dynamicImporters();
    },
    get importedIds() {
      return importedIds();
    },
    get dynamicallyImportedIds() {
      return dynamicallyImportedIds();
    },
    get exports() {
      return exports();
    },
    isEntry: info.isEntry,
    inputFormat: info.inputFormat,
    hasTopLevelAwait: info.hasTopLevelAwait,
    ...option,
  };
}
