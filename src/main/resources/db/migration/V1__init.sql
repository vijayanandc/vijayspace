CREATE TABLE modules (
    id BINARY(16) PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    label VARCHAR(255),
    storage_table VARCHAR(32) NOT NULL,
    status VARCHAR(32),
    version INT,
    slug VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE TABLE fields (
    id BINARY(16) PRIMARY KEY,
    module_id BINARY(16) NOT NULL,
    name VARCHAR(255) NOT NULL,
    label VARCHAR(255),
    data_type VARCHAR(32) NOT NULL,
    is_required BIT,
    is_unique BIT,
    default_value VARCHAR(255),
    max_length INT,
    column_slot VARCHAR(64) NOT NULL,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    CONSTRAINT fk_fields_module FOREIGN KEY (module_id) REFERENCES modules(id),
    CONSTRAINT uq_field_name UNIQUE (module_id, name)
);

CREATE TABLE module_data_1 (
    id BINARY(16) PRIMARY KEY,
    module_id BINARY(16) NOT NULL,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    col_str_01 VARCHAR(1024),
    col_str_02 VARCHAR(1024),
    col_str_03 VARCHAR(1024),
    col_str_04 VARCHAR(1024),
    col_str_05 VARCHAR(1024),
    CONSTRAINT fk_md1_module FOREIGN KEY (module_id) REFERENCES modules(id)
);
