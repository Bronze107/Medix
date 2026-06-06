export interface Media {
  id: string;
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
  display_variant_id: string | null;
  thumb_256: string | null;
  lqip: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
}

export interface MediaImportResult {
  id: string;
  path: string;
  success: boolean;
  error: string | null;
}
