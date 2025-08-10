package com.vijay.meta.repo;

import com.vijay.meta.model.ModuleEntity;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.Optional;
import java.util.UUID;

public interface ModuleRepository extends JpaRepository<ModuleEntity, UUID> {
    Optional<ModuleEntity> findBySlug(String slug);
}
