ALTER TABLE embeddings ADD COLUMN embedding BLOB;
DELETE FROM embeddings;
