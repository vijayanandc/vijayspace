package com.vijay.meta.service;

import com.vijay.meta.model.ModuleEntity;
import com.vijay.meta.model.StorageTable;
import com.vijay.meta.repo.ModuleRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.UUID;

@Service
@Transactional
public class ModuleService {

    private final ModuleRepository moduleRepository;

    public ModuleService(ModuleRepository moduleRepository) {
        this.moduleRepository = moduleRepository;
    }

    public ModuleEntity createModule(String name, String label, StorageTable storageTable, String slug) {
        ModuleEntity module = ModuleEntity.builder()
                .id(UUID.randomUUID())
                .name(name)
                .label(label)
                .storageTable(storageTable)
                .slug(slug)
                .build();
        return moduleRepository.save(module);
    }

    public List<ModuleEntity> getModules() {
        return moduleRepository.findAll();
    }

    public ModuleEntity getBySlug(String slug) {
        return moduleRepository.findBySlug(slug).orElseThrow();
    }
}
