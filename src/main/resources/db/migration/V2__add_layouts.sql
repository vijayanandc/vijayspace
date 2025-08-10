CREATE TABLE layouts (
    id BINARY(16) PRIMARY KEY,
    module_id BINARY(16) NOT NULL,
    name VARCHAR(255) NOT NULL,
    type VARCHAR(32) NOT NULL,
    definition_json TEXT,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    CONSTRAINT fk_layout_module FOREIGN KEY (module_id) REFERENCES modules(id)
);
