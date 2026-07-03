import { invoke } from "@tauri-apps/api/core";
import type { Media, MediaImportResult } from "@/types/media";
import type { Tag } from "@/types/tag";
import type { LineageGraph } from "@/types/lineage";
import type { Caption } from "@/types/caption";
import type { Collection } from "@/types/collection";
import type { LlamaServerStatus, GgufModelList, AutoDetect, EmbeddingInfo } from "@/types/ai";
import type { SavedFilter } from "@/types/search";
import type { ExportOptions } from "@/types/export";
import type { BrowseItem, BrowseVisibility } from "@/types/browse";
import type { ComfyWorkflow, ComfyWorkflowDetail } from "@/types/comfyui";

export function greet(name: string): Promise<string> {
  return invoke("greet", { name });
}

export function mediaImport(paths: string[]): Promise<MediaImportResult[]> {
  return invoke("media_import", { paths });
}

export function mediaList(
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
): Promise<Media[]> {
  return invoke("media_list", { sortBy, descending, offset, limit });
}

export function browseSearch(
  query: string,
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
  variantVisibility: BrowseVisibility = "representative",
): Promise<BrowseItem[]> {
  return invoke("browse_search", { query, sortBy, descending, offset, limit, variantVisibility });
}

export function browseListByCollection(
  collectionId: string,
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
  variantVisibility: BrowseVisibility = "representative",
): Promise<BrowseItem[]> {
  return invoke("browse_list_by_collection", { collectionId, sortBy, descending, offset, limit, variantVisibility });
}

export function browseList(
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
  variantVisibility: BrowseVisibility = "representative",
): Promise<BrowseItem[]> {
  return invoke("browse_list", {
    sortBy,
    descending,
    offset,
    limit,
    variantVisibility,
  });
}

export function mediaThumbnail(id: string): Promise<string> {
  return invoke("media_thumbnail", { id });
}

export interface ThumbnailResult {
  id: string;
  path: string;
}

export function mediaThumbnailBatch(ids: string[]): Promise<ThumbnailResult[]> {
  return invoke("media_thumbnail_batch", { ids });
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

export function mediaTagRemoveBatch(mediaIds: string[], tagId: string): Promise<void> {
  return invoke("media_tag_remove_batch", { mediaIds, tagId });
}

export function mediaTagsClear(mediaId: string): Promise<void> {
  return invoke("media_tags_clear", { mediaId });
}

export function mediaTagsIntersect(mediaIds: string[]): Promise<Tag[]> {
  return invoke("media_tags_intersect", { mediaIds });
}

// --- Collections ---

export function collectionList(): Promise<Collection[]> {
  return invoke("collection_list");
}

export function collectionGet(id: string): Promise<Collection | null> {
  return invoke("collection_get", { id });
}

export function collectionCreate(name: string, description: string): Promise<string> {
  return invoke("collection_create", { name, description });
}

export function collectionDelete(id: string): Promise<void> {
  return invoke("collection_delete", { id });
}

export function collectionRename(id: string, name: string): Promise<void> {
  return invoke("collection_rename", { id, name });
}

export function collectionPin(id: string): Promise<void> {
  return invoke("collection_pin", { id });
}

export function collectionUnpin(id: string): Promise<void> {
  return invoke("collection_unpin", { id });
}

export function collectionAddItem(collectionId: string, mediaId: string): Promise<void> {
  return invoke("collection_add_item", { collectionId, mediaId });
}

export function collectionAddBatch(collectionId: string, mediaIds: string[]): Promise<void> {
  return invoke("collection_add_batch", { collectionId, mediaIds });
}

export function collectionRemoveItem(collectionId: string, mediaId: string): Promise<void> {
  return invoke("collection_remove_item", { collectionId, mediaId });
}

export function mediaListByCollection(
  collectionId: string,
  sortBy: string,
  descending: boolean,
  offset: number = 0,
  limit: number = 500,
): Promise<Media[]> {
  return invoke("media_list_by_collection", { collectionId, sortBy, descending, offset, limit });
}

export function collectionGetItemIds(collectionId: string): Promise<string[]> {
  return invoke("collection_get_item_ids", { collectionId });
}

export function collectionFirstMediaId(collectionId: string): Promise<string | null> {
  return invoke("collection_first_media_id", { collectionId });
}

// --- Search ---

export function mediaSearch(
  query: string,
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
): Promise<Media[]> {
  return invoke("media_search", { query, sortBy, descending, offset, limit });
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

export function captionCreateBatch(mediaIds: string[], text: string): Promise<void> {
  return invoke("caption_create_batch", { mediaIds, text });
}

// --- AI / Models ---

export function llamaServerStatus(): Promise<LlamaServerStatus> {
  return invoke("llama_server_status");
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

export function embeddingDelete(mediaId: string): Promise<void> {
  return invoke("embedding_delete", { mediaId });
}

export function embeddingClearAll(): Promise<string> {
  return invoke("embedding_clear_all");
}

export function embeddingServerStatus(): Promise<{
  running: boolean;
  port: number;
  pid: number | null;
}> {
  return invoke("embedding_server_status");
}

export function embeddingRebuildAll(): Promise<string> {
  return invoke("embedding_rebuild_all");
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

export function testProxy(proxyUrl: string): Promise<string> {
  return invoke("test_proxy", { proxyUrl });
}

// --- AI Image Generation ---

export interface StagedImage {
  id: string;
  path: string;
  width: number;
  height: number;
  file_size: number;
}

export function imageGenerate(
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
): Promise<StagedImage[]> {
  return invoke("image_generate", { prompt, aspectRatio, resolution, n });
}

export function imageDiscardStaged(stagedIds: string[]): Promise<void> {
  return invoke("image_discard_staged", { stagedIds });
}

// --- Image Queue ---

export interface ImageTaskInfo {
  task_id: string;
  task_type: string; // "generate" | "edit"
  prompt: string;
  media_id: string | null;
  status: string; // "pending" | "running" | "done" | "failed"
  staged: StagedImage[];
  error: string | null;
  created_at: string;
}

export function imageQueueSubmitGenerate(
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
  workflowId?: string | null,
): Promise<string> {
  return invoke("image_queue_submit_generate", { prompt, aspectRatio, resolution, n, workflowId: workflowId ?? null });
}

export async function imageQueueSubmitEdit(
  sourceMediaIds: string[],
  prompt: string,
  aspectRatio?: string,
  resolution?: string,
  n?: number,
  workflowId?: string | null,
): Promise<string> {
  return invoke("image_queue_submit_edit", {
    sourceMediaIds,
    prompt,
    aspectRatio: aspectRatio || "auto",
    resolution: resolution || "1k",
    n: n || 1,
    workflowId: workflowId || null,
  });
}

export function imageQueueList(): Promise<ImageTaskInfo[]> {
  return invoke("image_queue_list");
}

export function imageQueuePendingCount(): Promise<number> {
  return invoke("image_queue_pending_count");
}

export function imageQueueImport(
  taskId: string,
  selectedIds: string[],
): Promise<MediaImportResult[]> {
  return invoke("image_queue_import", { taskId, selectedIds });
}

export function imageQueueDiscard(taskId: string): Promise<void> {
  return invoke("image_queue_discard", { taskId });
}

export function imageQueueDismiss(taskId: string): Promise<void> {
  return invoke("image_queue_dismiss", { taskId });
}

// --- Lineage ---

export async function mediaLineageList(mediaId: string): Promise<LineageGraph> {
  return invoke("media_lineage_list", { mediaId });
}

export async function mediaLineageAdd(
  parentId: string,
  childId: string,
  relationType: string,
): Promise<void> {
  return invoke("media_lineage_add", { parentId, childId, relationType });
}

export async function mediaLineageRemove(
  parentId: string,
  childId: string,
): Promise<void> {
  return invoke("media_lineage_remove", { parentId, childId });
}

export async function mediaListRootsCount(): Promise<number> {
  return invoke("media_list_roots_count");
}

// --- ComfyUI ---

export function comfyuiWorkflowList(workflowType?: string): Promise<ComfyWorkflow[]> {
  return invoke("comfyui_workflow_list", { workflowType: workflowType ?? null });
}

export function comfyuiWorkflowGet(id: string): Promise<ComfyWorkflowDetail> {
  return invoke("comfyui_workflow_get", { id });
}

export function comfyuiWorkflowCreate(
  name: string,
  workflowType: string,
  workflowJson: string,
): Promise<ComfyWorkflow> {
  return invoke("comfyui_workflow_create", { name, workflowType, workflowJson });
}

export function comfyuiWorkflowUpdate(
  id: string,
  name: string,
  workflowJson: string,
): Promise<ComfyWorkflow> {
  return invoke("comfyui_workflow_update", { id, name, workflowJson });
}

export function comfyuiWorkflowDelete(id: string): Promise<void> {
  return invoke("comfyui_workflow_delete", { id });
}

export function comfyuiTestConnection(): Promise<string> {
  return invoke("comfyui_test_connection");
}
