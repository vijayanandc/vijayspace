package com.vijay.data.repo;

import com.vijay.data.model.ModuleData1Entity;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.UUID;

public interface ModuleData1Repository extends JpaRepository<ModuleData1Entity, UUID> {
}
