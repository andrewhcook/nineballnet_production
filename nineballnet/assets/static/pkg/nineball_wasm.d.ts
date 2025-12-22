/* tslint:disable */
/* eslint-disable */

export function run_game(canvas_id: string, gateway_url: string, handoff_token: string): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly run_game: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly wgpu_render_bundle_draw: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly wgpu_render_bundle_draw_indexed: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly wgpu_render_bundle_set_pipeline: (a: number, b: bigint) => void;
  readonly wgpu_render_bundle_draw_indirect: (a: number, b: bigint, c: bigint) => void;
  readonly wgpu_render_bundle_set_bind_group: (a: number, b: number, c: bigint, d: number, e: number) => void;
  readonly wgpu_render_bundle_set_vertex_buffer: (a: number, b: number, c: bigint, d: bigint, e: bigint) => void;
  readonly wgpu_render_bundle_set_push_constants: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly wgpu_render_bundle_draw_indexed_indirect: (a: number, b: bigint, c: bigint) => void;
  readonly wgpu_render_bundle_insert_debug_marker: (a: number, b: number) => void;
  readonly wgpu_render_bundle_pop_debug_group: (a: number) => void;
  readonly wgpu_render_bundle_set_index_buffer: (a: number, b: bigint, c: number, d: bigint, e: bigint) => void;
  readonly wgpu_render_bundle_push_debug_group: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h2f78ba2d10ff3a2d: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h648124f5ccb916e4: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h0d26f778afe4d440: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h19702a69851894c4: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hbac0acc1adf59861: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h5f8cf5c1ce1eb0db: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h95336bcd11dd760f: (a: number, b: number) => void;
  readonly wasm_bindgen__closure__destroy__h258e951ddd50f47c: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__he33ab63df9443194: (a: number, b: number, c: any, d: any) => void;
  readonly wasm_bindgen__convert__closures_____invoke__he0d172362c95ddc2: (a: number, b: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
