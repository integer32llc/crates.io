ALTER TABLE crates
  ADD COLUMN namespace_id INTEGER;

ALTER TABLE crates
  ADD CONSTRAINT fk_namespace_id__crate_id
  FOREIGN KEY (namespace_id) REFERENCES crates (id);
