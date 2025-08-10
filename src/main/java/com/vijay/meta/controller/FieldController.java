package com.vijay.meta.controller;

import com.vijay.meta.model.DataType;
import com.vijay.meta.model.FieldEntity;
import com.vijay.meta.service.FieldService;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/v1/meta/modules/{slug}/fields")
public class FieldController {

    private final FieldService fieldService;

    public FieldController(FieldService fieldService) {
        this.fieldService = fieldService;
    }

    @PostMapping
    public FieldEntity addField(@PathVariable String slug, @RequestBody FieldEntity request) {
        return fieldService.addField(slug, request.getName(), request.getLabel(),
                request.getDataType() == null ? DataType.STRING : request.getDataType(),
                request.isRequired());
    }

    @GetMapping
    public List<FieldEntity> list(@PathVariable String slug) {
        return fieldService.listFields(slug);
    }
}
