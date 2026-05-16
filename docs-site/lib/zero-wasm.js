let browserCompilerPromise = null;
let browserCompilerCacheKey = null;

export async function runZero(command, source, target) {
  if (command === "auto") {
    return runZeroAuto(source, target);
  }

  return runZeroCommand(command, source, target);
}

async function runZeroAuto(source, target) {
  const wasmResult = await runZeroCommand("emit-wasm", source, target);
  if (wasmResult.ok || diagnosticCode(wasmResult) !== "CGEN004") return wasmResult;

  const checkResult = await runZeroCommand("check", source, target);
  if (!checkResult.ok) return checkResult;

  return {
    ...checkResult,
    execution: unsupportedExecution("check-fallback"),
    message: "Check passed. Execution is unavailable for this source in the browser compiler.",
  };
}

async function runZeroCommand(command, source, target) {
  const request = { schemaVersion: 1, command, source, target };
  const compilerCacheKey = playgroundCompilerCacheKey(source);
  let browserCompiler = null;
  try {
    browserCompiler = await loadBrowserCompiler(compilerCacheKey);
  } catch (error) {
    return {
      ok: false,
      compilerLoader: {
        primary: "zeroc-zero.wasm",
        compatibility: "removed",
        compatibilityUsed: false,
        request,
        artifact: null,
      },
      diagnostics: [{
        code: "AGT002",
        message: `Playground compiler is not built yet. Run npm run docs:wasm and refresh. (${error instanceof Error ? error.message : "unknown error"})`,
      }],
      message: "Playground compiler is unavailable.",
    };
  }

  return runBrowserCompile(browserCompiler, request);
}

function diagnosticCode(result) {
  return result?.diagnostic?.code ?? result?.diagnostics?.[0]?.code ?? null;
}

function unsupportedExecution(reason) {
  return {
    ok: false,
    status: "unsupported",
    code: "RUN001",
    reason,
    message: "The browser compiler accepted this source but did not return executable Wasm bytes for it.",
    expected: "executable wasm bytes from zeroc-zero.wasm",
    actual: "compile and artifact-plan metadata only",
  };
}

async function runBrowserCompile(browserCompiler, request) {
  const source = request.source ?? "";
  const sourceBytes = new TextEncoder().encode(source);
  const manifestBytes = new TextEncoder().encode(request.manifest ?? "{\"package\":{\"name\":\"playground\",\"version\":\"0.1.0\"},\"targets\":{\"cli\":{\"kind\":\"exe\",\"main\":\"src/main.0\"}}}");
  let acceptedBuffer = true;
  let tokenSummary = null;
  let sourceHash = null;
  let eofLocation = null;
  let manifestSummary = null;
  let moduleImportSummary = null;
  let moduleImportDiag = null;
  let compileRequest = null;
  let compileResponse = null;
  let artifactPlan = null;
  let writtenWasmBytes = 0;
  let wasmBase64 = null;
  if (browserCompiler.instance.exports.memory && typeof browserCompiler.instance.exports.compiler_accept_source_buffer === "function") {
    const sourcePtr = 1024;
    const manifestPtr = 4096;
    const artifactPtr = 49152;
    const artifactLen = 8192;
    const memory = new Uint8Array(browserCompiler.instance.exports.memory.buffer);
    acceptedBuffer = sourcePtr + sourceBytes.length <= memory.length && manifestPtr + manifestBytes.length <= memory.length;
    if (acceptedBuffer) {
      memory.set(sourceBytes, sourcePtr);
      memory.set(manifestBytes, manifestPtr);
      acceptedBuffer = browserCompiler.instance.exports.compiler_accept_source_buffer(sourcePtr, sourceBytes.length, memory.length) === 63;
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_token_summary === "function") {
        tokenSummary = browserCompiler.instance.exports.compiler_token_summary(sourcePtr, sourceBytes.length, memory.length);
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_source_hash === "function") {
        sourceHash = browserCompiler.instance.exports.compiler_source_hash(sourcePtr, sourceBytes.length, memory.length) >>> 0;
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_source_location_at === "function") {
        eofLocation = browserCompiler.instance.exports.compiler_source_location_at(sourcePtr, sourceBytes.length, memory.length, sourceBytes.length);
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_module_import_summary === "function") {
        moduleImportSummary = browserCompiler.instance.exports.compiler_module_import_summary(sourcePtr, sourceBytes.length, memory.length, 1);
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_module_import_diag_code === "function") {
        moduleImportDiag = browserCompiler.instance.exports.compiler_module_import_diag_code(sourcePtr, sourceBytes.length, memory.length, 1);
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_manifest_summary === "function") {
        manifestSummary = browserCompiler.instance.exports.compiler_manifest_summary(manifestPtr, manifestBytes.length, memory.length);
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_browser_compile_request === "function") {
        compileRequest = browserCompiler.instance.exports.compiler_browser_compile_request(
          sourcePtr,
          sourceBytes.length,
          manifestPtr,
          manifestBytes.length,
          memory.length,
          browserTargetId(request.target),
          browserCommandId(request.command),
          browserOutputId(request.command)
        );
      }
      if (acceptedBuffer && typeof browserCompiler.instance.exports.compiler_browser_artifact_plan === "function") {
        artifactPlan = browserCompiler.instance.exports.compiler_browser_artifact_plan(sourcePtr, sourceBytes.length, memory.length, browserTargetId(request.target), browserOutputId(request.command));
      }
      if (acceptedBuffer && compileRequest && artifactPlan && typeof browserCompiler.instance.exports.compiler_browser_compile_response === "function") {
        const parseSummary = typeof browserCompiler.instance.exports.compiler_parse_ast_summary === "function"
          ? browserCompiler.instance.exports.compiler_parse_ast_summary(sourcePtr, sourceBytes.length, memory.length)
          : 1;
        const checkSummary = typeof browserCompiler.instance.exports.compiler_type_surface_summary === "function"
          ? browserCompiler.instance.exports.compiler_type_surface_summary(sourcePtr, sourceBytes.length, memory.length)
          : 1;
        const mirSummary = typeof browserCompiler.instance.exports.compiler_mir_lowering_summary === "function"
          ? browserCompiler.instance.exports.compiler_mir_lowering_summary(sourcePtr, sourceBytes.length, memory.length)
          : 1;
        compileResponse = browserCompiler.instance.exports.compiler_browser_compile_response(parseSummary || 1, checkSummary || 1, mirSummary || 1, artifactPlan);
      }
      if (
        acceptedBuffer &&
        request.command === "emit-wasm" &&
        typeof browserCompiler.instance.exports.compiler_self_host_write_playground_wasm === "function" &&
        artifactPtr + artifactLen <= memory.length &&
        sourcePtr + sourceBytes.length <= artifactPtr
      ) {
        writtenWasmBytes = browserCompiler.instance.exports.compiler_self_host_write_playground_wasm(
          sourcePtr,
          sourceBytes.length,
          artifactPtr,
          artifactLen,
          memory.length
        );
        if (writtenWasmBytes > 0) {
          wasmBase64 = bytesToBase64(memory.slice(artifactPtr, artifactPtr + writtenWasmBytes));
        }
      }
    }
  }
  const accepted = acceptedBuffer && (compileRequest === null || compileRequest > 0);
  const execution = accepted && request.command === "emit-wasm" ? unsupportedExecution("artifact-plan-only") : null;
  const result = {
    ok: accepted,
    compilerLoader: {
      primary: "zeroc-zero.wasm",
      compatibility: "removed",
      compatibilityUsed: false,
      request,
      artifact: browserCompiler.artifact,
      tokenSummary,
      sourceHash,
      eofLocation,
      manifestSummary,
      moduleImportSummary,
      moduleImportDiag,
      compileRequest,
      compileResponse,
      artifactPlan,
      writtenWasmBytes,
    },
    diagnostics: accepted ? [] : [{ code: "AGT001", message: "source is outside the current playground compiler request contract" }],
    emittedArtifact: accepted && request.command === "emit-wasm"
      ? { kind: "wasm", bytes: writtenWasmBytes || artifactPlan, source: "zeroc-zero.wasm", executable: writtenWasmBytes > 0 }
      : null,
    execution: writtenWasmBytes > 0 ? null : execution,
    wasmBase64,
    message: accepted
      ? writtenWasmBytes > 0
        ? "Playground compiler emitted and ran Wasm bytes."
        : execution ? "Playground compiler accepted the request; execution is unavailable until this source has emitted Wasm bytes." : "Playground compiler accepted and compiled the request."
      : "Playground compiler returned an unsupported-source diagnostic.",
  };
  return writtenWasmBytes > 0 ? await hydrateWasmResult(result) : result;
}

function browserTargetId(target) {
  if (target === "wasm32-web") return 1;
  if (target === "wasm32-wasi") return 2;
  return 3;
}

function browserCommandId(command) {
  if (command === "check") return 1;
  if (command === "emit-wasm") return 2;
  return 3;
}

function browserOutputId(command) {
  if (command === "emit-wasm") return 3;
  return 1;
}

export async function hydrateWasmResult(result) {
  if (!result?.ok || !result.wasmBase64) return result;

  const bytes = base64ToBytes(result.wasmBase64);
  const hydrated = {
    ...result,
    wasmBytes: bytes.byteLength,
  };

  try {
    let memory = null;
    const output = { stdout: "", stderr: "" };
    const imports = createPlaygroundWasiImports(() => memory, output);
    const { instance } = await WebAssembly.instantiate(bytes, imports);
    memory = instance.exports.memory instanceof WebAssembly.Memory ? instance.exports.memory : null;
    const main = instance.exports.main;
    hydrated.wasmExports = Object.keys(instance.exports);
    if (typeof main === "function" && main.length === 0) {
      hydrated.wasmRun = {
        ok: true,
        result: main(),
        stdout: output.stdout,
        stderr: output.stderr,
      };
      if (output.stdout.length > 0) hydrated.stdout = output.stdout;
      if (output.stderr.length > 0) hydrated.stderr = output.stderr;
    } else if (typeof main === "function") {
      hydrated.wasmRun = {
        ok: false,
        reason: `main expects ${main.length} argument${main.length === 1 ? "" : "s"}`,
      };
    }
  } catch (error) {
    hydrated.wasmRun = {
      ok: false,
      reason: error instanceof Error ? error.message : "failed to instantiate wasm",
    };
  }

  return hydrated;
}

function createPlaygroundWasiImports(getMemory, output) {
  function writeOutput(fd, iovs, iovsLen, writtenPtr) {
    const memory = getMemory();
    if (!memory) return 8;
    const view = new DataView(memory.buffer);
    const bytes = new Uint8Array(memory.buffer);
    let written = 0;
    let text = "";

    for (let i = 0; i < iovsLen; i++) {
      const base = iovs + i * 8;
      const ptr = view.getUint32(base, true);
      const len = view.getUint32(base + 4, true);
      text += new TextDecoder().decode(bytes.subarray(ptr, ptr + len));
      written += len;
    }

    if (fd === 2) output.stderr += text;
    else output.stdout += text;
    if (writtenPtr) view.setUint32(writtenPtr, written, true);
    return 0;
  }

  return {
    wasi_snapshot_preview1: {
      fd_write: writeOutput,
      fd_read() { return 8; },
      fd_close() { return 0; },
      path_open() { return 76; },
      args_sizes_get(argcPtr, argvBufSizePtr) {
        const memory = getMemory();
        if (!memory) return 8;
        const view = new DataView(memory.buffer);
        view.setUint32(argcPtr, 0, true);
        view.setUint32(argvBufSizePtr, 0, true);
        return 0;
      },
      args_get() { return 0; },
      environ_sizes_get(countPtr, sizePtr) {
        const memory = getMemory();
        if (!memory) return 8;
        const view = new DataView(memory.buffer);
        view.setUint32(countPtr, 0, true);
        view.setUint32(sizePtr, 0, true);
        return 0;
      },
      environ_get() { return 0; },
      clock_time_get(_id, _precision, outPtr) {
        const memory = getMemory();
        if (!memory) return 8;
        new DataView(memory.buffer).setBigUint64(outPtr, 0n, true);
        return 0;
      },
      random_get(ptr, len) {
        const memory = getMemory();
        if (!memory) return 8;
        new Uint8Array(memory.buffer).fill(0, ptr, ptr + len);
        return 0;
      },
      fd_filestat_get() { return 8; },
      fd_readdir() { return 8; },
      path_create_directory() { return 76; },
      path_remove_directory() { return 76; },
      path_unlink_file() { return 76; },
      path_rename() { return 76; },
    },
  };
}

function base64ToBytes(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function bytesToBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

async function loadBrowserCompiler(cacheKey) {
  if (!browserCompilerPromise || browserCompilerCacheKey !== cacheKey) {
    browserCompilerCacheKey = cacheKey;
    browserCompilerPromise = fetchPlaygroundArtifact("zeroc-zero.wasm", cacheKey)
      .then(async (response) => {
        if (!response.ok) throw new Error(`${response.status} ${response.url}`);
        const bytes = await response.arrayBuffer();
        const { instance } = await WebAssembly.instantiate(bytes, {});
        if (typeof instance.exports.compiler_accept_source_buffer !== "function") {
          throw new Error("browser compiler API missing");
        }
        await verifyBrowserCompiler(instance);
        return {
          instance,
          artifact: {
            path: "playground/zeroc-zero.wasm",
            bytes: bytes.byteLength,
            exports: Object.keys(instance.exports),
          },
        };
      })
      .catch((error) => {
        browserCompilerPromise = null;
        browserCompilerCacheKey = null;
        throw error;
      });
  }

  return browserCompilerPromise;
}

async function verifyBrowserCompiler(instance) {
  if (
    !instance?.exports?.memory ||
    typeof instance.exports.compiler_self_host_write_playground_wasm !== "function"
  ) {
    throw new Error("browser compiler playground ABI missing");
  }

  const canarySource = `fun answer() -> i32 {
    return 40 + 2
}

pub fun main(world: World) -> Void raises {
    let value = answer()
    if value == 43 {
        check world.out.write("math works\\n")
    } else {
        check world.out.write("math broke\\n")
    }
}
`;
  const sourceBytes = new TextEncoder().encode(canarySource);
  const sourcePtr = 1024;
  const artifactPtr = 49152;
  const artifactLen = 8192;
  const memory = new Uint8Array(instance.exports.memory.buffer);
  if (sourcePtr + sourceBytes.length > memory.length || artifactPtr + artifactLen > memory.length) {
    throw new Error("browser compiler memory is too small for playground canary");
  }

  memory.set(sourceBytes, sourcePtr);
  const written = instance.exports.compiler_self_host_write_playground_wasm(
    sourcePtr,
    sourceBytes.length,
    artifactPtr,
    artifactLen,
    memory.length
  );
  if (written <= 0) {
    throw new Error("browser compiler failed the playground branch canary");
  }

  let runtimeMemory = null;
  const output = { stdout: "", stderr: "" };
  const imports = createPlaygroundWasiImports(() => runtimeMemory, output);
  const { instance: canaryInstance } = await WebAssembly.instantiate(
    memory.slice(artifactPtr, artifactPtr + written),
    imports
  );
  runtimeMemory = canaryInstance.exports.memory instanceof WebAssembly.Memory
    ? canaryInstance.exports.memory
    : null;
  if (typeof canaryInstance.exports.main !== "function" || canaryInstance.exports.main.length !== 0) {
    throw new Error("browser compiler emitted a non-runnable playground canary");
  }

  canaryInstance.exports.main();
  if (output.stdout !== "math broke\n") {
    throw new Error("browser compiler is stale; refresh the playground compiler artifact");
  }
}

async function fetchPlaygroundArtifact(path, cacheKey) {
  for (const url of playgroundArtifactUrls(path, cacheKey)) {
    const response = await fetch(url, { cache: "no-store" });
    if (response.ok) return response;
  }
  return fetch(playgroundArtifactUrl(path, cacheKey), { cache: "no-store" });
}

function playgroundArtifactUrl(path, cacheKey) {
  return playgroundArtifactUrls(path, cacheKey)[0];
}

function playgroundArtifactUrls(path, cacheKey) {
  const urls = [];
  if (typeof globalThis.location?.origin === "string") {
    urls.push(playgroundArtifactCacheBust(new URL(`/playground/${path}`, globalThis.location.origin), cacheKey));
  }
  urls.push(playgroundArtifactCacheBust(new URL(`../public/playground/${path}`, import.meta.url), cacheKey));
  return urls;
}

function playgroundArtifactCacheBust(url, cacheKey) {
  url.searchParams.set("zeroCompiler", cacheKey);
  return url;
}

function playgroundCompilerCacheKey(source) {
  let hash = 2166136261;
  for (let i = 0; i < source.length; i++) {
    hash ^= source.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return `zero-${source.length.toString(36)}-${(hash >>> 0).toString(36)}`;
}
