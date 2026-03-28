// Wrapper header for bindgen — pulls in the FidelityFX unified API and the
// feature-specific headers for upscaling and frame generation.
//
// ffx_api_dx12.h is intentionally excluded because it pulls in d3d12.h which
// drags in the entire Windows SDK. The DX12 backend descriptor is defined
// manually in Rust.

// Neutralise __declspec(dllexport) before any SDK header can define it.
// ffx_api.h unconditionally does `#define FFX_API_ENTRY __declspec(dllexport)`,
// overriding any prior #define of FFX_API_ENTRY, so we must disable __declspec
// itself to prevent clang (without MSVC target) from choking on it.
#define __declspec(x)

#include "ffx_api.h"
#include "ffx_upscale.h"
#include "ffx_framegeneration.h"
