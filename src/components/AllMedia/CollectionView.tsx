import { useParams } from "react-router";
import AllMedia from "./AllMedia";

function CollectionView() {
  const { id } = useParams<{ id: string }>();
  if (!id) return null;
  return <AllMedia collectionId={id} />;
}

export default CollectionView;
