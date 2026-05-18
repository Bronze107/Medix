import { invoke } from "@tauri-apps/api/core";
import type { Media, MediaImportResult } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { Variant, VariantPreset } from "@/types/variant";
import type { Caption } from "@/types/caption";
import type { LlamaServerStatus, GgufModelList, AutoDetect, EmbeddingInfo } from "@/types/ai";
import type { SavedFilter } from "@/types/search";
import type { ExportOptions } from "@/types/export";

export function greet(name: string): Promise<string> {
  return invoke("greet", { name });
}

export function mediaImport(paths: string[]): Promise<MediaImportResult[]> {
  return invoke("media_import", { paths });
}

export function mediaList(
  sortBy: string = "imported_at",
  descending: boolean = true
): Promise<Media[]> {
  return invoke("media_list", { sortBy, descending });
}

export function mediaThumbnail(id: string): Promise<string> {
  return invoke("media_thumbnail", { id });
}

export function mediaSoftDelete(id: string): Promise<void> {
  return invoke("media_soft_delete", { id });
}

export function mediaRecover(id: string): Promise<void> {
  return invoke("media_recover", { id });
}

export function mediaPermanentDelete(id: string): Promise<void> {
  return invoke("media_permanent_delete", { id });
}

export function mediaListTrash(
  sortBy: string = "imported_at",
  descending: boolean = true
): Promise<Media[]> {
  return invoke("media_list_trash", { sortBy, descending });
}

export function mediaEmptyTrash(): Promise<number> {
  return invoke("media_empty_trash");
}

export function mediaFindDuplicates(): Promise<Media[][]> {
  return invoke("media_find_duplicates");
}

export interface MediaPaths {
  original: string | null;
  thumb_256: string | null;
  thumb_512: string | null;
}

export function mediaGetPaths(id: string): Promise<MediaPaths> {
  return invoke("media_get_paths", { id });
}

export function mediaAiAnnotate(id: string): Promise<void> {
  return invoke("media_ai_annotate", { id });
}

// --- Tags ---

export function tagList(): Promise<Tag[]> {
  return invoke("tag_list");
}

export function tagCreate(name: string): Promise<string> {
  return invoke("tag_create", { name });
}

export function tagDelete(id: string): Promise<void> {
  return invoke("tag_delete", { id });
}

export function tagRename(id: string, name: string): Promise<void> {
  return invoke("tag_rename", { id, name });
}

export function mediaTagsGet(mediaId: string): Promise<Tag[]> {
  return invoke("media_tags_get", { mediaId });
}

export function mediaTagAdd(mediaId: string, tagId: string): Promise<void> {
  return invoke("media_tag_add", { mediaId, tagId });
}

export function mediaTagAddBatch(mediaIds: string[], tagId: string): Promise<void> {
  return invoke("media_tag_add_batch", { mediaIds, tagId });
}

export function mediaTagRemove(mediaId: string, tagId: string): Promise<void> {
  return invoke("media_tag_remove", { mediaId, tagId });
}

// --- Search ---

export function mediaSearch(
  query: string,
  sortBy: string = "imported_at",
  descending: boolean = true
): Promise<Media[]> {
  return invoke("media_search", { query, sortBy, descending });
}

// --- Variants ---

export function variantList(mediaId: string): Promise<Variant[]> {
  return invoke("variant_list", { mediaId });
}

export function variantGenerate(
  mediaId: string,
  label: string,
  format: string,
  maxWidth: number | null,
  maxHeight: number | null,
  quality: number,
): Promise<Variant> {
  return invoke("variant_generate", { mediaId, label, format, maxWidth, maxHeight, quality });
}

export function variantImport(
  mediaId: string,
  sourcePath: string,
): Promise<Variant> {
  return invoke("variant_import", { mediaId, sourcePath });
}

export function variantDelete(id: string): Promise<void> {
  return invoke("variant_delete", { id });
}

export function variantPresets(): Promise<VariantPreset[]> {
  return invoke("variant_presets");
}

// --- Captions ---

export function captionList(mediaId: string): Promise<Caption[]> {
  return invoke("caption_list", { mediaId });
}

export function captionCreate(mediaId: string, text: string): Promise<Caption> {
  return invoke("caption_create", { mediaId, text });
}

export function captionUpdate(id: string, text: string): Promise<void> {
  return invoke("caption_update", { id, text });
}

export function captionDelete(id: string): Promise<void> {
  return invoke("caption_delete", { id });
}

// --- AI / Models ---

export function llamaServerStatus(): Promise<LlamaServerStatus> {
  return invoke("llama_server_status");
}

export function llamaServerStart(): Promise<void> {
  return invoke("llama_server_start");
}

export function llamaServerStop(): Promise<void> {
  return invoke("llama_server_stop");
}

export function modelList(): Promise<GgufModelList> {
  return invoke("model_list");
}

export function autoDetect(): Promise<AutoDetect> {
  return invoke("auto_detect");
}

export function embeddingInfo(mediaId: string): Promise<EmbeddingInfo[]> {
  return invoke("embedding_info", { mediaId });
}

export function aiPendingCount(): Promise<number> {
  return invoke("ai_pending_count");
}

// --- Saved Filters ---

export function savedFiltersList(): Promise<SavedFilter[]> {
  return invoke("saved_filters_list");
}

export function savedFiltersSave(name: string, query: string): Promise<void> {
  return invoke("saved_filters_save", { name, query });
}

export function savedFiltersDelete(name: string): Promise<void> {
  return invoke("saved_filters_delete", { name });
}

// --- Export ---

export function exportDataset(options: ExportOptions): Promise<string> {
  return invoke("export_dataset", { options });
}

export function importZip(zipPath: string): Promise<number> {
  return invoke("import_zip", { zipPath });
}

// --- Settings ---

export function settingsGet(key: string): Promise<string | null> {
  return invoke("settings_get", { key });
}

export function settingsSet(key: string, value: string): Promise<void> {
  return invoke("settings_set", { key, value });
}

export function settingsGetAll(): Promise<Record<string, string>> {
  return invoke("settings_get_all");
}
