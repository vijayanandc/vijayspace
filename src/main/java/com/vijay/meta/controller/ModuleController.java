package com.vijay.meta.controller;

import com.vijay.meta.model.ModuleEntity;
import com.vijay.meta.model.StorageTable;
import com.vijay.meta.service.ModuleService;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/v1/meta/modules")
public class ModuleController {

    private final ModuleService moduleService;

    public ModuleController(ModuleService moduleService) {
        this.moduleService = moduleService;
    }

    @PostMapping
    public ResponseEntity<ModuleEntity> create(@RequestBody ModuleEntity request) {
        ModuleEntity created = moduleService.createModule(
                request.getName(),
                request.getLabel(),
                request.getStorageTable() == null ? StorageTable.MODULE_DATA_1 : request.getStorageTable(),
                request.getSlug()
        );
        return ResponseEntity.ok(created);
    }

    @GetMapping
    public List<ModuleEntity> list() {
        return moduleService.getModules();
    }

    @GetMapping("/{slug}")
    public ModuleEntity get(@PathVariable String slug) {
        return moduleService.getBySlug(slug);
    }
}
