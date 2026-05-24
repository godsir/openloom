import { createSessionFileResourceEnvelope } from "../resources/resource-envelope.js";

export function serializeSessionFile(file, options = {}) {
  if (!file) return null;
  const id = file.id || file.fileId || null;
  const studioId = resolveStudioId(options);
  const resource = studioId
    ? createSessionFileResourceEnvelope({ ...file, ...(id ? { id } : {}) }, { studioId })
    : null;
  return {
    ...(id ? { id, fileId: id } : {}),
    ...(file.sessionPath ? { sessionPath: file.sessionPath } : {}),
    filePath: file.filePath,
    ...(file.realPath ? { realPath: file.realPath } : {}),
    ...(file.displayName ? { displayName: file.displayName } : {}),
    ...(file.filename ? { filename: file.filename } : {}),
    ...(file.label ? { label: file.label } : {}),
    ...(file.ext !== undefined ? { ext: file.ext } : {}),
    ...(file.mime ? { mime: file.mime } : {}),
    ...(file.size !== undefined ? { size: file.size } : {}),
    ...(file.kind ? { kind: file.kind } : {}),
    ...(file.isDirectory !== undefined ? { isDirectory: file.isDirectory } : {}),
    ...(file.origin ? { origin: file.origin } : {}),
    ...(Array.isArray(file.operations) ? { operations: file.operations } : {}),
    ...(file.createdAt !== undefined ? { createdAt: file.createdAt } : {}),
    ...(file.storageKind ? { storageKind: file.storageKind } : {}),
    ...(file.status ? { status: file.status } : {}),
    ...(file.missingAt !== undefined ? { missingAt: file.missingAt } : {}),
    ...(resource ? { resource } : {}),
  };
}

export function registerSessionFileFromRequest(engine, { sessionPath, filePath, label, origin, storageKind }) {
  if (!sessionPath) return null;
  if (typeof engine?.registerSessionFile !== "function") {
    throw new Error("session file registry unavailable");
  }
  return serializeSessionFile(engine.registerSessionFile({
    sessionPath,
    filePath,
    label,
    origin,
    storageKind,
  }), { runtimeContext: safeRuntimeContext(engine) });
}

function resolveStudioId(options = {}) {
  if (typeof options.studioId === "string" && options.studioId.trim()) return options.studioId;
  if (typeof options.runtimeContext?.studioId === "string" && options.runtimeContext.studioId.trim()) {
    return options.runtimeContext.studioId;
  }
  return null;
}

function safeRuntimeContext(engine) {
  try {
    if (typeof engine?.getRuntimeContext === "function") return engine.getRuntimeContext();
  } catch {}
  return engine?.runtimeContext || null;
}
