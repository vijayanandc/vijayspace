package com.vijay.meta.repo;

import com.vijay.meta.model.FieldEntity;
import com.vijay.meta.model.ModuleEntity;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.List;
import java.util.UUID;

public interface FieldRepository extends JpaRepository<FieldEntity, UUID> {
    List<FieldEntity> findByModule(ModuleEntity module);
}
