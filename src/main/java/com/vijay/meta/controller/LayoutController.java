package com.vijay.meta.controller;

import com.vijay.meta.model.LayoutEntity;
import com.vijay.meta.model.LayoutType;
import com.vijay.meta.service.LayoutService;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/v1/meta/modules/{slug}/layouts")
public class LayoutController {

    private final LayoutService layoutService;

    public LayoutController(LayoutService layoutService) {
        this.layoutService = layoutService;
    }

    @PostMapping
    public LayoutEntity create(@PathVariable String slug, @RequestBody LayoutEntity request) {
        return layoutService.create(slug, request.getName(),
                request.getType() == null ? LayoutType.FORM : request.getType(),
                request.getDefinitionJson());
    }

    @GetMapping
    public List<LayoutEntity> list(@PathVariable String slug) {
        return layoutService.list(slug);
    }
}
