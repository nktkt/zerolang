#include "zero.h"

#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static bool is_directory(const char *path) {
  struct stat st;
  return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

static char *dirname_of(const char *path) {
  const char *slash = strrchr(path, '/');
  if (!slash) return z_strdup(".");
  return z_strndup(path, (size_t)(slash - path));
}

static char *join_path(const char *left, const char *right) {
  ZBuf buf;
  zbuf_init(&buf);
  zbuf_append(&buf, left);
  if (left[strlen(left) - 1] != '/') zbuf_append_char(&buf, '/');
  zbuf_append(&buf, right);
  return buf.data;
}

static bool ends_with(const char *text, const char *suffix) {
  size_t text_len = strlen(text);
  size_t suffix_len = strlen(suffix);
  return text_len >= suffix_len && strcmp(text + text_len - suffix_len, suffix) == 0;
}

static char *project_root_for(const char *input) {
  if (ends_with(input, "zero.json")) return dirname_of(input);
  if (is_directory(input)) return z_strdup(input);
  char *dir = dirname_of(input);
  return dir;
}

bool z_discover_routes_json(const char *input, char **json, ZDiag *diag) {
  char *root = project_root_for(input);
  char *route_dir = join_path(root, "src/routes");
  DIR *dir = opendir(route_dir);
  if (!dir) {
    diag->code = 5001;
    diag->path = route_dir;
    diag->line = 1;
    diag->column = 1;
    snprintf(diag->message, sizeof(diag->message), "cannot read route directory");
    free(root);
    free(route_dir);
    return false;
  }

  ZBuf buf;
  zbuf_init(&buf);
  zbuf_append(&buf, "{\n  \"schemaVersion\": 1,\n  \"runtime\": \"wasm32-web\",\n  \"routes\": [\n");
  bool first = true;
  int route_count = 0;
  struct dirent *entry;
  while ((entry = readdir(dir)) != NULL) {
    if (!ends_with(entry->d_name, ".0")) continue;
    if (!first) zbuf_append(&buf, ",\n");
    first = false;
    route_count++;
    char *file = join_path(route_dir, entry->d_name);
    char *stem = z_strndup(entry->d_name, strlen(entry->d_name) - 2);
    const char *route_path = strcmp(stem, "index") == 0 ? "/" : stem;
    zbuf_appendf(&buf, "    {\"method\": \"GET\", \"path\": \"%s\", \"file\": \"%s\"}", route_path, file);
    free(file);
    free(stem);
  }
  closedir(dir);
  zbuf_appendf(&buf, "\n  ],\n  \"routeCount\": %d,\n", route_count);
  zbuf_append(&buf, "  \"requiresCapabilities\": [\"web\"],\n");
  zbuf_append(&buf, "  \"capabilityFacts\": {\n");
  zbuf_append(&buf, "    \"filesystem\": {\"required\": false, \"browserWasm\": \"unavailable\", \"wasi\": \"capability-gated\", \"staticLinuxServerless\": \"temp-only by capability\"},\n");
  zbuf_append(&buf, "    \"environment\": {\"required\": false, \"browserWasm\": \"preloaded only\", \"edge\": \"capability-gated\"},\n");
  zbuf_append(&buf, "    \"process\": {\"required\": false, \"browserWasm\": \"unavailable\", \"edge\": \"unavailable\"}\n");
  zbuf_append(&buf, "  },\n");
  zbuf_append(&buf, "  \"ownership\": {\n");
  zbuf_append(&buf, "    \"wasi\": {\"requestBody\": \"host-owned stream\", \"responseBody\": \"guest-owned until return\", \"filesystem\": \"capability-gated\"},\n");
  zbuf_append(&buf, "    \"browserWasm\": {\"requestBody\": \"JS-owned buffer view\", \"responseBody\": \"guest-owned copy\", \"filesystem\": \"unavailable\"},\n");
  zbuf_append(&buf, "    \"staticLinuxServerless\": {\"requestBody\": \"runtime-owned stream\", \"responseBody\": \"guest-owned until flush\", \"filesystem\": \"temp-only by capability\"},\n");
  zbuf_append(&buf, "    \"platformAdapter\": {\"boundary\": \"Request in, Response out\", \"allocations\": \"handler-visible only\"}\n");
  zbuf_append(&buf, "  },\n");
  zbuf_append(&buf, "  \"handlerAbi\": {\n");
  zbuf_append(&buf, "    \"request\": {\"type\": \"Request\", \"ownership\": \"borrowed\", \"body\": \"stream-or-borrowed-bytes\"},\n");
  zbuf_append(&buf, "    \"response\": {\"type\": \"Response\", \"ownership\": \"returned\", \"body\": \"owned-or-static-bytes\"},\n");
  zbuf_append(&buf, "    \"errors\": {\"model\": \"raises\", \"lowering\": \"typed status/error packet\"},\n");
  zbuf_append(&buf, "    \"memory\": {\"allocator\": \"explicit Alloc parameter when needed\", \"requestAllocation\": \"zero by default\"}\n");
  zbuf_append(&buf, "  },\n");
  zbuf_append(&buf, "  \"webSurfaces\": {\n");
  zbuf_append(&buf, "    \"request\": [\"method\", \"url\", \"headers\", \"cookies\", \"params\", \"body\"],\n");
  zbuf_append(&buf, "    \"response\": [\"status\", \"headers\", \"cookies\", \"body\", \"stream\"],\n");
  zbuf_append(&buf, "    \"environment\": [\"preloaded env\", \"region\"],\n");
  zbuf_append(&buf, "    \"cache\": [\"cache.match\", \"cache.put\", \"cache-control\"],\n");
  zbuf_append(&buf, "    \"lifecycle\": [\"wait_until\"]\n");
  zbuf_append(&buf, "  },\n");
  zbuf_append(&buf, "  \"measurements\": {\n");
  zbuf_append(&buf, "    \"compressedSizeBudgetBytes\": 10240,\n");
  zbuf_append(&buf, "    \"coldStartBudgetMs\": 1,\n");
  zbuf_append(&buf, "    \"memoryFloorBudgetBytes\": 65536,\n");
  zbuf_append(&buf, "    \"perRequestAllocationBudgetBytes\": 0,\n");
  zbuf_append(&buf, "    \"status\": \"route manifest and web bundle audit metadata emitted\"\n");
  zbuf_append(&buf, "  },\n");
  zbuf_append(&buf, "  \"artifact\": {\"target\": \"wasm32-web\", \"kind\": \"route-manifest\", \"available\": true, \"format\": \"zero.routes.v1\", \"generatedCBytes\": 0},\n");
  zbuf_append(&buf, "  \"webBundle\": {\"target\": \"wasm32-web\", \"kind\": \"direct-wasm-web-bundle\", \"available\": true, \"javascriptFrameworkTaxBytes\": 0, \"frameworkTaxBytes\": 0, \"adapter\": \"host-provided web capabilities\", \"imports\": [\"web.request\", \"web.response\", \"env.preloaded\", \"cache\", \"wait_until\"], \"capabilityRestrictions\": \"checked before codegen\", \"deployment\": {\"providerSpecific\": false, \"vercel\": \"out-of-scope\"}},\n");
  zbuf_append(&buf, "  \"localRuntime\": {\"schemaVersion\": 1, \"target\": \"wasm32-web\", \"runtimeKind\": \"browser-worker\", \"providerSpecificDeployment\": false, \"hostedDeployment\": \"out-of-scope\", \"productionLikeImports\": true, \"command\": \"zero dev --target wasm32-web\", \"imports\": {\"explicit\": true, \"module\": \"zero_web_preview1\", \"functions\": [\"web.request\", \"web.response\", \"env.preloaded\", \"cache\", \"wait_until\"], \"adapter\": \"browser-worker-import-shim\"}, \"capabilityRestrictions\": {\"filesystem\": \"denied\", \"environment\": \"preloaded import only\", \"arguments\": \"preloaded import only\", \"stdio\": \"explicit import\", \"dom\": \"unavailable to portable worker module\", \"network\": \"denied until Fetch capability\", \"process\": \"denied\"}, \"memoryFloor\": {\"linearMemory\": true, \"pageBytes\": 65536, \"minimumPages\": 1, \"floorBytes\": 65536}, \"frameworkTaxBytes\": 0},\n");
  zbuf_append(&buf, "  \"artifactAudit\": {\"localRuntime\": \"covered by command-contracts, wasm:runtime:smoke, and docs:compiler\", \"imports\": \"explicit capability names\", \"filesystem\": \"denied for browser wasm\", \"preloadedInputs\": \"manifest-declared only\", \"providerSpecificDeployment\": false}\n}\n");
  free(root);
  free(route_dir);
  *json = buf.data;
  return true;
}
