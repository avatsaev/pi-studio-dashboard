import type { Backend, Dialog, Reply } from "./types";

/** Run a registry command by id (`engine.execute`): the only way the frontend changes anything. */
export function execute(backend: Backend, command: string, params?: Record<string, unknown>): Promise<Reply> {
  return backend.control("engine.execute", { command, params: params ?? null });
}

/** Open or close a dialog: `UiState.dialog` is open/close state only (app-structure § UiState). */
export function setDialog(backend: Backend, dialog: Dialog | null): Promise<Reply> {
  return backend.control("ui.set", { dialog });
}

export function setDiagnosticsTarget(backend: Backend, entryId: string | null): Promise<Reply> {
  return backend.control("ui.set", { diagnosticsTarget: entryId });
}

export interface FieldError {
  field: string;
  code: string;
  message: string;
}

/**
 * A command's `bad params: {"errors":[{field,code,message}]}` as field errors, first per field.
 * Anything else comes back as `rest`: text for the dialog's own message line.
 */
export function fieldErrors(error: string): { fields: Record<string, string>; rest: string | null } {
  const start = error.indexOf("{");
  if (start !== -1) {
    try {
      const parsed: unknown = JSON.parse(error.slice(start));
      if (parsed && typeof parsed === "object" && "errors" in parsed && Array.isArray(parsed.errors)) {
        const fields: Record<string, string> = {};
        for (const item of parsed.errors as unknown[]) {
          if (item && typeof item === "object" && "field" in item && "message" in item && typeof item.field === "string" && typeof item.message === "string" && !(item.field in fields)) {
            fields[item.field] = item.message;
          }
        }
        return { fields, rest: null };
      }
    } catch {
      // Not JSON: show the text as it is.
    }
  }
  return { fields: {}, rest: error };
}
