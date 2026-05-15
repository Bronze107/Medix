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
  thumb_256: string | null;
  thumb_512: string | null;
}

export interface MediaImportResult {
  id: string;
  path: string;
  success: boolean;
  error: string | null;
}
