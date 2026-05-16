#!/usr/bin/env node
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const outDir = ".zero/wasm-runtime-smoke";
const maxBuffer = 16 * 1024 * 1024;

await mkdir(outDir, { recursive: true });

async function zeroJson(args) {
  const result = await execFileAsync("bin/zero", args, { maxBuffer });
  return JSON.parse(result.stdout);
}

async function buildWasm({ target, input, name }) {
  const outBase = join(outDir, name);
  await rm(`${outBase}.wasm`, { force: true });
  await rm(`${outBase}.wasm.c`, { force: true });
  const body = await zeroJson(["build", "--json", "--emit", "wasm", "--target", target, input, "--out", outBase]);
  assert.equal(body.generatedCBytes, 0);
  assert.equal(body.cBridgeFallback ?? false, false);
  assert.equal(body.objectBackend.objectEmission.path, "direct-wasm");
  const bytes = await readFile(`${outBase}.wasm`);
  assert.equal(bytes[0], 0x00);
  assert.equal(bytes[1], 0x61);
  assert.equal(bytes[2], 0x73);
  assert.equal(bytes[3], 0x6d);
  return { body, bytes, path: `${outBase}.wasm` };
}

function makeRuntime({ args = [], env = [], files = new Map(), dirs = new Set(["."]), expectedOutput }) {
  let instance;
  let output = "";
  const fds = new Map();
  let nextFd = 4;

  function view() {
    return new DataView(instance.exports.memory.buffer);
  }

  function memory() {
    return new Uint8Array(instance.exports.memory.buffer);
  }

  function readI32(ptr) {
    return view().getUint32(ptr, true);
  }

  function writeI32(ptr, value) {
    view().setUint32(ptr, value >>> 0, true);
  }

  function writeU64(ptr, value) {
    view().setBigUint64(ptr, BigInt(value), true);
  }

  function readString(ptr, len) {
    return Buffer.from(instance.exports.memory.buffer, ptr, len).toString("utf8");
  }

  function byteLength(items) {
    return items.reduce((total, item) => total + Buffer.byteLength(item) + 1, 0);
  }

  function writeStrings(ptrArray, ptrBytes, items) {
    let offset = ptrBytes;
    for (let i = 0; i < items.length; i++) {
      const bytes = Buffer.from(items[i]);
      writeI32(ptrArray + i * 4, offset);
      memory().set(bytes, offset);
      memory()[offset + bytes.length] = 0;
      offset += bytes.length + 1;
    }
  }

  function writeFileAt(entry, chunks) {
    const incoming = Buffer.concat(chunks);
    const current = files.get(entry.path) || Buffer.alloc(0);
    const end = entry.pos + incoming.length;
    const next = Buffer.alloc(Math.max(current.length, end));
    current.copy(next, 0, 0, current.length);
    incoming.copy(next, entry.pos);
    files.set(entry.path, next);
    entry.pos = end;
  }

  const imports = {
    wasi_snapshot_preview1: {
      args_sizes_get(argcPtr, argvBufSizePtr) {
        writeI32(argcPtr, args.length);
        writeI32(argvBufSizePtr, byteLength(args));
        return 0;
      },
      args_get(argvPtr, argvBufPtr) {
        writeStrings(argvPtr, argvBufPtr, args);
        return 0;
      },
      environ_sizes_get(countPtr, envBufSizePtr) {
        writeI32(countPtr, env.length);
        writeI32(envBufSizePtr, byteLength(env));
        return 0;
      },
      environ_get(envPtr, envBufPtr) {
        writeStrings(envPtr, envBufPtr, env);
        return 0;
      },
      path_open(fd, dirflags, pathPtr, pathLen, oflags, rightsBase, rightsInheriting, fdflags, openedFdPtr) {
        const path = readString(pathPtr, pathLen);
        const rights = BigInt(rightsBase);
        const directory = (oflags & 2) !== 0;
        const write = (oflags & 9) !== 0 || (rights & 64n) !== 0n;
        if (directory) {
          if (!dirs.has(path)) return 44;
          const opened = nextFd++;
          fds.set(opened, { type: "dir", path, pos: 0 });
          writeI32(openedFdPtr, opened);
          return 0;
        }
        if (write) files.set(path, Buffer.alloc(0));
        if (!files.has(path)) return 44;
        const opened = nextFd++;
        fds.set(opened, { type: "file", path, pos: 0 });
        writeI32(openedFdPtr, opened);
        return 0;
      },
      fd_read(fd, iovs, iovsLen, nread) {
        const entry = fds.get(fd);
        if (!entry || entry.type !== "file") return 8;
        const source = files.get(entry.path) || Buffer.alloc(0);
        let total = 0;
        for (let i = 0; i < iovsLen; i++) {
          const ptr = readI32(iovs + i * 8);
          const len = readI32(iovs + i * 8 + 4);
          const chunk = source.subarray(entry.pos, entry.pos + len);
          memory().set(chunk, ptr);
          entry.pos += chunk.length;
          total += chunk.length;
        }
        writeI32(nread, total);
        return 0;
      },
      fd_write(fd, iovs, iovsLen, nwritten) {
        let total = 0;
        const chunks = [];
        for (let i = 0; i < iovsLen; i++) {
          const ptr = readI32(iovs + i * 8);
          const len = readI32(iovs + i * 8 + 4);
          chunks.push(Buffer.from(instance.exports.memory.buffer, ptr, len));
          total += len;
        }
        if (fd === 1 || fd === 2) {
          output += Buffer.concat(chunks).toString("utf8");
        } else {
          const entry = fds.get(fd);
          if (!entry || entry.type !== "file") return 8;
          writeFileAt(entry, chunks);
        }
        writeI32(nwritten, total);
        return 0;
      },
      fd_close(fd) {
        fds.delete(fd);
        return 0;
      },
      fd_filestat_get(fd, buf) {
        const entry = fds.get(fd);
        if (!entry || entry.type !== "file") return 8;
        writeU64(buf + 32, (files.get(entry.path) || Buffer.alloc(0)).length);
        return 0;
      },
      fd_readdir(fd, buf, bufLen, cookie, nread) {
        const entry = fds.get(fd);
        if (!entry || entry.type !== "dir") return 8;
        writeI32(nread, 0);
        return 0;
      },
      path_create_directory(fd, pathPtr, pathLen) {
        dirs.add(readString(pathPtr, pathLen));
        return 0;
      },
      path_remove_directory(fd, pathPtr, pathLen) {
        dirs.delete(readString(pathPtr, pathLen));
        return 0;
      },
      path_unlink_file(fd, pathPtr, pathLen) {
        const path = readString(pathPtr, pathLen);
        if (!files.has(path)) return 44;
        files.delete(path);
        return 0;
      },
      path_rename(oldFd, oldPtr, oldLen, newFd, newPtr, newLen) {
        const oldPath = readString(oldPtr, oldLen);
        const newPath = readString(newPtr, newLen);
        if (!files.has(oldPath)) return 44;
        files.set(newPath, files.get(oldPath));
        files.delete(oldPath);
        return 0;
      },
    },
  };

  return {
    imports,
    files,
    setInstance(next) {
      instance = next;
    },
    assertOutput() {
      assert.equal(output, expectedOutput);
    },
  };
}

function instantiateAndRun(bytes, runtime) {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes), runtime.imports);
  runtime.setInstance(instance);
  assert.equal(instance.exports.main(), 0);
  runtime.assertOutput();
  return instance;
}

const argsBuild = await buildWasm({
  target: "wasm32-wasi",
  input: "conformance/native/pass/std-args.0",
  name: "std-args-wasi",
});
instantiateAndRun(argsBuild.bytes, makeRuntime({
  args: ["zero", "agent-arg", "extra"],
  expectedOutput: "agent-arg\n",
}));

const envBuild = await buildWasm({
  target: "wasm32-wasi",
  input: "conformance/native/pass/std-env.0",
  name: "std-env-wasi",
});
instantiateAndRun(envBuild.bytes, makeRuntime({
  env: ["ZERO_CONFORMANCE_ENV=agent-env", "OTHER=value"],
  expectedOutput: "env ok\n",
}));

const webEnvBuild = await buildWasm({
  target: "wasm32-web",
  input: "conformance/native/pass/std-env.0",
  name: "std-env-web",
});
instantiateAndRun(webEnvBuild.bytes, makeRuntime({
  env: ["ZERO_CONFORMANCE_ENV=agent-env", "OTHER=value"],
  expectedOutput: "env ok\n",
}));

const fsBuild = await buildWasm({
  target: "wasm32-wasi",
  input: "conformance/native/pass/std-fs-resource.0",
  name: "std-fs-resource-wasi",
});
const fsRuntime = makeRuntime({
  files: new Map(),
  expectedOutput: "fs resource ok\n",
});
instantiateAndRun(fsBuild.bytes, fsRuntime);
assert.equal(fsRuntime.files.get(".zero/conformance/std-fs-resource.txt")?.toString("utf8"), "zero file\n");

const addBuild = await buildWasm({
  target: "wasm32-web",
  input: "examples/direct-wasm-add.0",
  name: "direct-wasm-add-web",
});
const addInstance = new WebAssembly.Instance(new WebAssembly.Module(addBuild.bytes), {});
assert.equal(addInstance.exports.main(40, 2), 42);

const wasiSize = await zeroJson(["size", "--json", "--target", "wasm32-wasi", "conformance/native/pass/std-fs-resource.0"]);
assert.equal(wasiSize.portableRuntime.runtimeKind, "wasi");
assert.equal(wasiSize.portableRuntime.providerSpecificDeployment, false);
assert.equal(wasiSize.portableRuntime.hostedDeployment, "out-of-scope");
assert.equal(wasiSize.portableRuntime.localRunner.productionLikeImports, true);
for (const importName of ["fd_write", "fd_read", "fd_close", "path_open"]) {
  assert(wasiSize.portableRuntime.imports.functions.includes(importName), `${importName} should be imported for WASI fs`);
}
assert.equal(wasiSize.portableRuntime.memoryFloor.floorBytes, 65536);
assert.equal(wasiSize.runtimeImportAudit.memoryFloor.floorBytes, 65536);

const webSize = await zeroJson(["size", "--json", "--target", "wasm32-web", "conformance/native/pass/std-env.0"]);
assert.equal(webSize.portableRuntime.runtimeKind, "browser-worker");
assert.equal(webSize.portableRuntime.providerSpecificDeployment, false);
assert.equal(webSize.portableRuntime.imports.adapter, "browser-worker-import-shim");
assert(webSize.portableRuntime.imports.functions.includes("environ_get"));
assert(webSize.portableRuntime.imports.functions.includes("fd_write"));
assert.equal(webSize.portableRuntime.capabilityRestrictions.filesystem, "denied");
assert.equal(webSize.portableRuntime.frameworkTaxBytes, 0);

const devPlan = await zeroJson(["dev", "--json", "--target", "wasm32-web", "examples/web/hello"]);
assert.equal(devPlan.localRuntime.runtimeKind, "browser-worker");
assert.equal(devPlan.localRuntime.productionLikeImports, true);
assert.equal(devPlan.localRuntime.providerSpecificDeployment, false);
assert.equal(devPlan.localRuntime.capabilityRestrictions.filesystem, "denied");
assert.equal(devPlan.localRuntime.frameworkTaxBytes, 0);

const routes = await zeroJson(["routes", "--json", "examples/web/hello"]);
assert.equal(routes.localRuntime.runtimeKind, "browser-worker");
assert.equal(routes.localRuntime.providerSpecificDeployment, false);
assert.equal(routes.localRuntime.productionLikeImports, true);
assert.equal(routes.webBundle.deployment.providerSpecific, false);
assert.equal(routes.webBundle.deployment.vercel, "out-of-scope");
assert.equal(routes.localRuntime.frameworkTaxBytes, 0);

console.log("wasm runtime smoke ok");
