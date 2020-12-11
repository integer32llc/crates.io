ALTER TABLE crates
  DROP CONSTRAINT fk_namespace_id__crate_id;

ALTER TABLE crates
  DROP COLUMN namespace_id;
