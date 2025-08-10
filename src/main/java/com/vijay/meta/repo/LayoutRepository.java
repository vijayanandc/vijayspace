package com.vijay.meta.repo;

import com.vijay.meta.model.LayoutEntity;
import com.vijay.meta.model.ModuleEntity;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.List;
import java.util.UUID;

public interface LayoutRepository extends JpaRepository<LayoutEntity, UUID> {
    List<LayoutEntity> findByModule(ModuleEntity module);
}
