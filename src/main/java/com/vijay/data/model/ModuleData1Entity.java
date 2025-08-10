package com.vijay.data.model;

import jakarta.persistence.*;
import lombok.Getter;
import lombok.Setter;

import java.time.Instant;
import java.util.UUID;

@Entity
@Table(name = "module_data_1")
@Getter
@Setter
public class ModuleData1Entity {

    @Id
    @Column(columnDefinition = "BINARY(16)")
    private UUID id;

    @Column(name = "module_id", columnDefinition = "BINARY(16)", nullable = false)
    private UUID moduleId;

    @Column(name = "created_at")
    private Instant createdAt;

    @Column(name = "updated_at")
    private Instant updatedAt;

    // sample string slots
    @Column(name = "col_str_01", length = 1024)
    private String colStr01;

    @Column(name = "col_str_02", length = 1024)
    private String colStr02;

    @Column(name = "col_str_03", length = 1024)
    private String colStr03;

    @Column(name = "col_str_04", length = 1024)
    private String colStr04;

    @Column(name = "col_str_05", length = 1024)
    private String colStr05;

    @PrePersist
    public void prePersist() {
        Instant now = Instant.now();
        createdAt = now;
        updatedAt = now;
        if (id == null) {
            id = UUID.randomUUID();
        }
    }

    @PreUpdate
    public void preUpdate() {
        updatedAt = Instant.now();
    }
}
