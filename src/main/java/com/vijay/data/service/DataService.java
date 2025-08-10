package com.vijay.data.service;

import com.vijay.data.model.ModuleData1Entity;
import com.vijay.data.repo.ModuleData1Repository;
import com.vijay.meta.model.FieldEntity;
import com.vijay.meta.model.ModuleEntity;
import com.vijay.meta.model.StorageTable;
import com.vijay.meta.repo.FieldRepository;
import com.vijay.meta.repo.ModuleRepository;
import org.springframework.beans.BeanWrapper;
import org.springframework.beans.BeanWrapperImpl;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.*;

@Service
@Transactional
public class DataService {

    private final ModuleRepository moduleRepository;
    private final FieldRepository fieldRepository;
    private final ModuleData1Repository moduleData1Repository;

    public DataService(ModuleRepository moduleRepository, FieldRepository fieldRepository, ModuleData1Repository moduleData1Repository) {
        this.moduleRepository = moduleRepository;
        this.fieldRepository = fieldRepository;
        this.moduleData1Repository = moduleData1Repository;
    }

    public ModuleData1Entity createRecord(String moduleSlug, Map<String, Object> values) {
        ModuleEntity module = moduleRepository.findBySlug(moduleSlug).orElseThrow();
        if (module.getStorageTable() != StorageTable.MODULE_DATA_1) {
            throw new UnsupportedOperationException("Only MODULE_DATA_1 supported in this demo");
        }
        List<FieldEntity> fields = fieldRepository.findByModule(module);
        Map<String, FieldEntity> fieldMap = new HashMap<>();
        for (FieldEntity f : fields) {
            fieldMap.put(f.getName(), f);
        }
        ModuleData1Entity entity = new ModuleData1Entity();
        entity.setModuleId(module.getId());
        BeanWrapper wrapper = new BeanWrapperImpl(entity);
        for (Map.Entry<String, Object> e : values.entrySet()) {
            FieldEntity field = fieldMap.get(e.getKey());
            if (field == null) continue; // ignore unknown
            String slot = field.getColumnSlot();
            String prop = toCamel(slot);
            wrapper.setPropertyValue(prop, e.getValue());
        }
        return moduleData1Repository.save(entity);
    }

    public ModuleData1Entity getRecord(String moduleSlug, UUID id) {
        ModuleEntity module = moduleRepository.findBySlug(moduleSlug).orElseThrow();
        if (module.getStorageTable() != StorageTable.MODULE_DATA_1) {
            throw new UnsupportedOperationException("Only MODULE_DATA_1 supported in this demo");
        }
        return moduleData1Repository.findById(id).orElseThrow();
    }

    private String toCamel(String slot) {
        String[] parts = slot.split("_");
        StringBuilder sb = new StringBuilder(parts[0]);
        for (int i = 1; i < parts.length; i++) {
            sb.append(parts[i].substring(0, 1).toUpperCase()).append(parts[i].substring(1));
        }
        return sb.toString();
    }
}
