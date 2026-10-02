export type BrowseVisibility = "representative" | "all";

export interface BrowseItem {
  id: string;
  media_id: string;
  source_path: string | null;
  width: number | null;
  height: number | null;
  file_size: number | null;
  created_at: string | null;
  modified_at: string | null;
  imported_at: string;
  source_url: string | null;
  page_url: string | null;
  source: string | null;
  sha256: string | null;
  deleted_at: string | null;
  thumb_256: string | null;
  lqip: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
  child_count: number;
  parent_count: number;
}
