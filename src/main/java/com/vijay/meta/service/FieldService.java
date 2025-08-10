package com.vijay.meta.service;

import com.vijay.meta.model.*;
import com.vijay.meta.repo.FieldRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.*;

@Service
@Transactional
public class FieldService {

    private static final List<String> STRING_SLOTS = List.of(
            "col_str_01", "col_str_02", "col_str_03", "col_str_04", "col_str_05"
    );

    private final FieldRepository fieldRepository;
    private final ModuleService moduleService;

    public FieldService(FieldRepository fieldRepository, ModuleService moduleService) {
        this.fieldRepository = fieldRepository;
        this.moduleService = moduleService;
    }

    public FieldEntity addField(String moduleSlug, String name, String label, DataType dataType, boolean required) {
        ModuleEntity module = moduleService.getBySlug(moduleSlug);
        String slot = allocateSlot(module, dataType);
        FieldEntity field = FieldEntity.builder()
                .module(module)
                .name(name)
                .label(label)
                .dataType(dataType)
                .required(required)
                .columnSlot(slot)
                .build();
        return fieldRepository.save(field);
    }

    public List<FieldEntity> listFields(String moduleSlug) {
        ModuleEntity module = moduleService.getBySlug(moduleSlug);
        return fieldRepository.findByModule(module);
    }

    private String allocateSlot(ModuleEntity module, DataType dataType) {
        if (dataType != DataType.STRING) {
            throw new UnsupportedOperationException("Only STRING fields supported in this demo");
        }
        List<FieldEntity> fields = fieldRepository.findByModule(module);
        Set<String> used = new HashSet<>();
        for (FieldEntity f : fields) {
            used.add(f.getColumnSlot());
        }
        return STRING_SLOTS.stream()
                .filter(slot -> !used.contains(slot))
                .findFirst()
                .orElseThrow(() -> new IllegalStateException("No free slots"));
    }
}
