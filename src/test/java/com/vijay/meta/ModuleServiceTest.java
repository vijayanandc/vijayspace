package com.vijay.meta;

import com.vijay.meta.model.ModuleEntity;
import com.vijay.meta.model.StorageTable;
import com.vijay.meta.service.ModuleService;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.ActiveProfiles;

import static org.assertj.core.api.Assertions.assertThat;

@SpringBootTest
@ActiveProfiles("dev")
class ModuleServiceTest {

    @Autowired
    private ModuleService moduleService;

    @Test
    void createAndFetchModule() {
        ModuleEntity module = moduleService.createModule("test", "Test", StorageTable.MODULE_DATA_1, "test");
        ModuleEntity fetched = moduleService.getBySlug("test");
        assertThat(fetched.getName()).isEqualTo("test");
        assertThat(fetched.getId()).isEqualTo(module.getId());
    }
}
