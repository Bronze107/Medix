export interface LineageEdge {
  media_id: string;
  relation_type: string;
  workflow_id: string | null;
  created_at: string;
}

export interface LineageGraph {
  parents: LineageEdge[];
  children: LineageEdge[];
}
