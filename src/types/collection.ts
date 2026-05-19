export interface Collection {
  id: string;
  name: string;
  description: string | null;
  pinned_at: string | null;
  created_at: string;
  item_count: number | null;
}
