package com.vijay.data.controller;

import com.vijay.data.model.ModuleData1Entity;
import com.vijay.data.service.DataService;
import org.springframework.web.bind.annotation.*;

import java.util.Map;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/data/{moduleSlug}")
public class DataController {

    private final DataService dataService;

    public DataController(DataService dataService) {
        this.dataService = dataService;
    }

    @PostMapping
    public ModuleData1Entity create(@PathVariable String moduleSlug, @RequestBody Map<String, Object> payload) {
        return dataService.createRecord(moduleSlug, payload);
    }

    @GetMapping("/{id}")
    public ModuleData1Entity get(@PathVariable String moduleSlug, @PathVariable UUID id) {
        return dataService.getRecord(moduleSlug, id);
    }
}
