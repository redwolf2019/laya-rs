#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include "onnxruntime_c_api.h"
static const OrtApi *api;
static void check(OrtStatus *s) {
  if (s) { fprintf(stderr, "%s\n", api->GetErrorMessage(s)); api->ReleaseStatus(s); exit(1); }
}
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!lib) { fprintf(stderr, "%s\n", dlerror()); return 1; }
  const OrtApiBase *(*get)(void) = dlsym(lib, "OrtGetApiBase");
  if (!get) return 1;
  api = get()->GetApi(ORT_API_VERSION);
  if (!api) return 1;
  printf("ORT %s API %d\n", get()->GetVersionString(), ORT_API_VERSION);
  char **providers; int count;
  check(api->GetAvailableProviders(&providers, &count));
  for (int i=0; i<count; i++) printf("available provider: %s\n", providers[i]);
  int failed = count != 1 || strcmp(providers[0], "CPUExecutionProvider");
  check(api->ReleaseAvailableProviders(providers, count));
  OrtEnv *env; check(api->CreateEnv(ORT_LOGGING_LEVEL_WARNING, "probe", &env));
  const OrtEpDevice *const *devices; size_t n;
  check(api->GetEpDevices(env, &devices, &n));
  for (size_t i=0; i<n; i++) printf("device provider: %s\n", api->EpDevice_EpName(devices[i]));
  failed |= n != 1 || strcmp(api->EpDevice_EpName(devices[0]), "CPUExecutionProvider");
  api->ReleaseEnv(env);
  dlclose(lib);
  return failed;
}
