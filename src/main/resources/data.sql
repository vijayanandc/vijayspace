INSERT INTO modules(id, name, label, storage_table, status, version, slug, created_at, updated_at)
VALUES (UNHEX(REPLACE('00000000-0000-0000-0000-000000000001','-','')),
        'contacts','Contacts','MODULE_DATA_1','ACTIVE',1,'contacts',NOW(),NOW());

INSERT INTO fields(id,module_id,name,label,data_type,is_required,is_unique,column_slot,created_at,updated_at)
VALUES (UNHEX(REPLACE('00000000-0000-0000-0000-000000000101','-','')),
        UNHEX(REPLACE('00000000-0000-0000-0000-000000000001','-','')),
        'name','Name','STRING',1,0,'col_str_01',NOW(),NOW());
