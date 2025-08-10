package com.vijay.meta.service;

import com.vijay.meta.model.LayoutEntity;
import com.vijay.meta.model.LayoutType;
import com.vijay.meta.model.ModuleEntity;
import com.vijay.meta.repo.LayoutRepository;
import org.springframework.stereotype.Service;

import java.util.List;

@Service
public class LayoutService {

    private final LayoutRepository layoutRepository;
    private final ModuleService moduleService;

    public LayoutService(LayoutRepository layoutRepository, ModuleService moduleService) {
        this.layoutRepository = layoutRepository;
        this.moduleService = moduleService;
    }

    public LayoutEntity create(String moduleSlug, String name, LayoutType type, String definitionJson) {
        ModuleEntity module = moduleService.getBySlug(moduleSlug);
        LayoutEntity layout = LayoutEntity.builder()
                .module(module)
                .name(name)
                .type(type)
                .definitionJson(definitionJson)
                .build();
        return layoutRepository.save(layout);
    }

    public List<LayoutEntity> list(String moduleSlug) {
        ModuleEntity module = moduleService.getBySlug(moduleSlug);
        return layoutRepository.findByModule(module);
    }
}
