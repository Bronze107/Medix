import { invoke } from "@tauri-apps/api/core";
import type { Media, MediaImportResult } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { Variant, VariantPreset } from "@/types/variant";

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
  presetName: string
): Promise<Variant> {
  return invoke("variant_generate", { mediaId, presetName });
}

export function variantDelete(id: string): Promise<void> {
  return invoke("variant_delete", { id });
}

export function variantPresets(): Promise<VariantPreset[]> {
  return invoke("variant_presets");
}
